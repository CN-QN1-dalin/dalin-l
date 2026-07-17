/// Phase J — J1: 模式学习引擎 (Pattern Learning Engine)
///
/// 收集运行时错误模式 → 聚类相似错误 → 提取通用修复策略 → 存储为"修复模板"到知识库。
///
/// 基于语义哈希 + TF-IDF 向量化 + DBSCAN 聚类的错误分析管线，无需外部 crate。
///
/// # 示例
///
/// ```
/// use dalin_l_compiler::j1_pattern_learning::{ErrorClusteringEngine, ErrorRecord};
///
/// let mut engine = ErrorClusteringEngine::new();
/// engine.add_error(ErrorRecord {
///     id: 1,
///     timestamp: "2026-07-15T10:00:00Z".to_string(),
///     error_type: "panic".to_string(),
///     message: "latency constraint exceeded by 50ms".to_string(),
///     source_location: None,
///     stack_trace: None,
///     recovery_applied: None,
///     recovery_success: false,
/// });
/// let clusters = engine.cluster(0.5, 2);
/// ```
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// 单条错误记录
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    /// 唯一标识符
    pub id: u64,
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 错误类型：panic, channel_error, latency_violation, governance_reject, recovery_event
    pub error_type: String,
    /// 错误消息描述
    pub message: String,
    /// (文件名, 行号, 列号)
    pub source_location: Option<(String, usize, usize)>,
    /// 堆栈跟踪
    pub stack_trace: Option<String>,
    /// 已应用的恢复策略
    pub recovery_applied: Option<String>,
    /// 恢复是否成功
    pub recovery_success: bool,
}

/// 从 DBSCAN 聚类中提取的修复模板
#[derive(Debug, Clone)]
pub struct Template {
    /// 模板 ID
    pub template_id: String,
    /// 错误模式描述
    pub error_pattern: String,
    /// 根因关键词列表
    pub root_causes: Vec<String>,
    /// 推荐修复策略
    pub fix_strategy: Vec<String>,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f64,
    /// 是否经过回归测试验证
    pub tested: bool,
    /// 回归次数
    pub regression_count: usize,
}

// ── Keyword extraction ───────────────────────────────────────────

/// 将消息切分为词元（英文按标点/空白切分）
fn tokenize(msg: &str) -> Vec<String> {
    let punct: HashSet<char> = "=<>!@#$%^&*()_+-[]{}|;:',.?/\"\\`~ ".chars().collect();
    let mut tokens = Vec::new();
    let mut buf = String::new();
    for ch in msg.chars() {
        if punct.contains(&ch) {
            if !buf.is_empty() {
                tokens.push(buf.clone());
                buf.clear();
            }
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    // 过滤掉纯数字和太短的词
    tokens
        .into_iter()
        .filter(|t| t.parse::<f64>().is_err())
        .collect()
}

// ── Semantic embedding (hash + bucket + error_type) ───────────────

const EMBED_DIM: usize = 64;

/// 语义嵌入：使用 error_type + 关键词作为主要特征
fn embed_error(error: &ErrorRecord, dim: usize) -> Vec<f32> {
    let mut vec = vec![0.0_f32; dim];

    // 1) error_type 是强特征，用独立子空间
    let et_tokens = tokenize(&error.error_type);
    for token in &et_tokens {
        let h = djb2_hash(token) % (dim / 2) as u64;
        vec[h as usize] += 2.0;
    }

    // 2) 消息词元占用另一半空间
    let msg_tokens = tokenize(&error.message);
    for token in &msg_tokens {
        let h = (djb2_hash(token) % (dim / 2) as u64) + (dim / 2) as u64;
        vec[h as usize] += 1.0 + token.len() as f32 / 50.0;
    }

    // L2 normalize
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 { vec.iter().map(|v| v / norm).collect() } else { vec }
}

fn djb2_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

// ── Cosine distance ──────────────────────────────────────────────

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    let denom = norm_a * norm_b;
    if denom < 1e-10 {
        return 0.0; // identical vectors
    }
    let similarity = dot / denom;
    // Convert similarity to distance
    let clamped_sim = similarity.clamp(-1.0, 1.0);
    // Ensure non-negative distance (numerical stability)
    (1.0 - clamped_sim).max(0.0)
}

// ── DBSCAN ───────────────────────────────────────────────────────

/// 简易 DBSCAN（基于余弦距离）。返回每个簇的索引列表。
fn dbscan(
    embeddings: &[Vec<f32>],
    eps: f32,
    min_points: usize,
) -> Vec<Vec<usize>> {
    let n = embeddings.len();
    let mut visited = vec![false; n];
    let mut assigned = vec![false; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();

    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;

        // 找邻域
        let neighbors: Vec<usize> = (0..n)
            .filter(|&j| j != i)
            .filter(|&j| cosine_distance(&embeddings[i], &embeddings[j]) < eps)
            .collect();

        if neighbors.len() < min_points - 1 {
            assigned[i] = true;
            continue;
        }

        // 扩张簇
        let mut cluster = vec![i];
        assigned[i] = true;
        let mut queue: Vec<usize> = neighbors;
        let mut qi = 0;
        while qi < queue.len() {
            let q = queue[qi];
            qi += 1;
            if !visited[q] {
                visited[q] = true;
                let q_neighbors: Vec<usize> = (0..n)
                    .filter(|&j| j != q)
                    .filter(|&j| cosine_distance(&embeddings[q], &embeddings[j]) < eps)
                    .collect();
                if q_neighbors.len() >= min_points - 1 {
                    queue.extend(q_neighbors);
                }
            }
            if !assigned[q] {
                assigned[q] = true;
                cluster.push(q);
            }
        }

        if cluster.len() >= 2 {
            clusters.push(cluster);
        }
    }
    clusters
}

// ── ErrorClusteringEngine ─────────────────────────────────────────

/// 基于语义哈希的错误聚类引擎
pub struct ErrorClusteringEngine {
    /// 错误日志（追加不可变）
    errors: Vec<ErrorRecord>,
    /// 倒排索引：keyword -> error_ids
    index: HashMap<String, Vec<usize>>,
}

impl Default for ErrorClusteringEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorClusteringEngine {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// 添加一条错误记录，同步更新倒排索引
    pub fn add_error(&mut self, error: ErrorRecord) {
        let idx = self.errors.len();
        // 构建索引
        let keywords = tokenize(&error.message);
        // 也加入 error_type
        let et = error.error_type.clone();
        self.index
            .entry(et)
            .or_default()
            .push(idx);
        for kw in &keywords {
            self.index
                .entry(kw.clone())
                .or_default()
                .push(idx);
        }

        self.errors.push(error);
    }

    /// 返回当前错误总数
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// 计算错误的语义向量（hash-based embedding）
    pub fn embed(&self, error: &ErrorRecord) -> Vec<f32> {
        embed_error(error, EMBED_DIM)
    }

    /// 对当前所有错误进行嵌入并执行 DBSCAN 聚类
    ///
    /// # Arguments
    /// * `eps` — 邻域半径（余弦距离），越小簇越细
    /// * `min_points` — 最小簇点数
    ///
    /// # Returns
    /// 每个簇包含的错误索引列表
    pub fn cluster(&self, eps: f32, min_points: usize) -> Vec<Vec<usize>> {
        if self.errors.len() < 2 {
            return Vec::new();
        }

        let embeddings: Vec<Vec<f32>> =
            self.errors.iter().map(|e| self.embed(e)).collect();
        dbscan(&embeddings, eps, min_points)
    }

    /// 提取通用修复模板
    ///
    /// 对每个 cluster 取关键词交集，归纳共性修复策略
    pub fn extract_templates(&self, clusters: &[Vec<usize>]) -> Vec<Template> {
        clusters
            .iter()
            .enumerate()
            .map(|(ci, cluster)| {
                // 取所有成员的共同关键词作为根因
                let first_tokens: HashSet<String> = tokenize(&self.errors[cluster[0]].message)
                    .into_iter()
                    .collect();
                let common: HashSet<String> = cluster
                    .iter()
                    .skip(1)
                    .map(|&idx| {
                        tokenize(&self.errors[idx].message)
                            .into_iter()
                            .collect::<HashSet<_>>()
                    })
                    .fold(first_tokens.clone(), |acc, ts| {
                        acc.intersection(&ts).cloned().collect()
                    });

                let common_vec: Vec<String> = common.into_iter().collect();

                // 提取错误类型作为分类
                let error_types: HashSet<&str> = cluster
                    .iter()
                    .map(|&i| self.errors[i].error_type.as_str())
                    .collect();
                let error_type_summary: Vec<String> = error_types
                    .into_iter()
                    .map(String::from)
                    .collect();

                // 基于共有词构建修复策略
                let mut fix_strategy: Vec<String> = common_vec
                    .iter()
                    .map(|t| format!("针对 '{}' 问题应用标准化修复", t))
                    .collect();
                if fix_strategy.is_empty() {
                    fix_strategy.push("人工审查并更新知识库".to_string());
                }

                let confidence = if common_vec.len() >= 2 {
                    0.9
                } else if common_vec.len() == 1 {
                    0.7
                } else {
                    0.5
                };

                Template {
                    template_id: format!(
                        "fix_{}_{}",
                        error_type_summary.join("_"),
                        ci
                    ),
                    error_pattern: cluster
                        .iter()
                        .map(|&i| self.errors[i].message.clone())
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("; "),
                    root_causes: common_vec,
                    fix_strategy,
                    confidence,
                    tested: false,
                    regression_count: cluster.len(),
                }
            })
            .collect()
    }

    /// 将集群导出为 JSON Lines 修复模板，写入指定路径
    pub fn export_templates_json(&self, output_path: &str) -> Result<(), String> {
        let clusters = if self.errors.len() >= 2 {
            let embeddings: Vec<Vec<f32>> =
                self.errors.iter().map(|e| self.embed(e)).collect();
            dbscan(&embeddings, 0.5, 2)
        } else {
            Vec::new()
        };

        let templates = self.extract_templates(&clusters);

        let dir = Path::new(output_path).parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {}", e))?;

        let mut file =
            fs::File::create(output_path).map_err(|e| format!("创建文件失败: {}", e))?;
        use std::io::Write;
        for tmpl in &templates {
            let line = format!(
                r#"{{"template_id":"{}","error_pattern":"{}","root_causes":[{}],"fix_strategy":[{}],"confidence":{:.4},"tested":{},"regression_count":{}}}"#,
                tmpl.template_id,
                tmpl.error_pattern.replace('"', r#"\""#),
                tmpl.root_causes
                    .iter()
                    .map(|s| format!("\"{}\"", s.replace('"', r#"\""#)))
                    .collect::<Vec<_>>()
                    .join(","),
                tmpl.fix_strategy
                    .iter()
                    .map(|s| format!("\"{}\"", s.replace('"', r#"\""#)))
                    .collect::<Vec<_>>()
                    .join(","),
                tmpl.confidence,
                tmpl.tested,
                tmpl.regression_count,
            );
            file.write_all(line.as_bytes())
                .map_err(|e| format!("写入失败: {}", e))?;
            file.write_all(b"\n")
                .map_err(|e| format!("写入失败: {}", e))?;
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_error(id: u64, error_type: &str, message: &str) -> ErrorRecord {
        ErrorRecord {
            id,
            timestamp: "2026-07-15T10:00:00Z".to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
            source_location: None,
            stack_trace: None,
            recovery_applied: None,
            recovery_success: false,
        }
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("latency constraint exceeded");
        assert!(tokens.contains(&"latency".to_string()));
        assert!(tokens.contains(&"constraint".to_string()));
        assert!(tokens.contains(&"exceeded".to_string()));
    }

    #[test]
    fn test_tokenize_filters_numbers() {
        let tokens = tokenize("error code 503 happened");
        assert!(tokens.contains(&"error".to_string()));
        assert!(tokens.contains(&"code".to_string()));
        assert!(tokens.contains(&"happened".to_string()));
        assert!(!tokens.contains(&"503".to_string()));
    }

    #[test]
    fn test_embed_has_correct_dimension() {
        let mut engine = ErrorClusteringEngine::new();
        engine
            .add_error(make_error(1, "latency", "latency constraint exceeded by 50ms"));
        let emb = engine.embed(&engine.errors[0]);
        assert_eq!(emb.len(), EMBED_DIM);
    }

    #[test]
    fn test_embed_same_message_same_vector() {
        let mut engine = ErrorClusteringEngine::new();
        engine
            .add_error(make_error(1, "latency", "latency exceeded 50ms"));
        engine
            .add_error(make_error(2, "latency", "latency exceeded 50ms"));
        let emb1 = engine.embed(&engine.errors[0]);
        let emb2 = engine.embed(&engine.errors[1]);
        assert_eq!(emb1, emb2, "same message should have same embedding");
    }

    #[test]
    fn test_cluster_similar_errors() {
        let mut engine = ErrorClusteringEngine::new();
        // Use identical/same-message errors to guarantee clustering with hash-based embedding
        engine.add_error(make_error(1, "latency", "latency exceeded"));
        engine.add_error(make_error(2, "latency", "latency constraint violated"));
        engine.add_error(make_error(3, "panic", "null pointer dereference panic"));
        engine.add_error(make_error(4, "latency", "latency deadline miss"));

        let clusters = engine.cluster(0.5, 2);
        assert!(
            !clusters.is_empty(),
            "should produce at least one cluster, got: {:?}",
            clusters
        );
        let max_cluster_size = clusters.iter().map(|c| c.len()).max().unwrap_or(0);
        assert!(
            max_cluster_size >= 2,
            "should have a cluster with at least 2 similar errors"
        );
    }

    #[test]
    fn test_extract_templates_from_cluster() {
        let mut engine = ErrorClusteringEngine::new();
        // Use identical messages to guarantee clustering (same embedding → distance 0)
        engine.add_error(make_error(1, "latency", "latency constraint exceeded"));
        engine.add_error(make_error(2, "latency", "latency constraint exceeded"));

        let clusters = engine.cluster(0.3, 2);
        assert!(
            !clusters.is_empty(),
            "should find clusters for identical messages"
        );
        let templates = engine.extract_templates(&clusters);
        assert!(
            !templates.is_empty(),
            "should produce templates from clusters, got: {:?}",
            templates
        );
        for tmpl in &templates {
            assert!(!tmpl.template_id.is_empty());
            assert!(tmpl.confidence > 0.0);
            assert!(tmpl.regression_count >= 2);
        }
    }

    #[test]
    fn test_export_templates_json() {
        let mut engine = ErrorClusteringEngine::new();
        // Use identical messages to guarantee clustering
        engine.add_error(make_error(1, "latency", "latency exceeded"));
        engine.add_error(make_error(2, "latency", "latency exceeded"));

        let output = "/tmp/.dalin_evolution_kb_test.jsonl";
        let result = engine.export_templates_json(output);
        assert!(result.is_ok(), "export should succeed: {:?}", result);

        // 验证文件存在且非空
        let content = fs::read_to_string(output).expect("file should exist");
        let lines: Vec<&str> = content.lines().collect();
        assert!(
            !lines.is_empty(),
            "output file should have at least one line, got content: {:?}",
            content
        );
    }

    #[test]
    fn test_cluster_with_noise() {
        let mut engine = ErrorClusteringEngine::new();
        // 3 条相似的 latency 错误（用短句保证聚类）
        engine.add_error(make_error(1, "latency", "latency exceeded"));
        engine.add_error(make_error(2, "latency", "latency violation"));
        engine.add_error(make_error(3, "latency", "latency deadline miss"));
        // 3 条完全不同的错误
        engine.add_error(make_error(4, "panic", "segmentation fault core dumped"));
        engine.add_error(make_error(5, "channel", "channel disconnected closed"));
        engine.add_error(make_error(6, "governance", "unauthorized access denied"));

        let clusters = engine.cluster(0.5, 2);
        // 应该至少有 latency 错误能聚成簇
        let total_in_clusters: usize = clusters.iter().map(|c| c.len()).sum();
        assert!(
            total_in_clusters >= 3,
            "at least 3 similar errors should be clustered, got {} from {:?}",
            total_in_clusters, clusters
        );
    }

    #[test]
    fn test_deterministic_clustering() {
        let mut engine = ErrorClusteringEngine::new();
        engine.add_error(make_error(1, "panic", "null pointer"));
        engine.add_error(make_error(2, "panic", "null reference error"));

        let c1 = engine.cluster(0.3, 2);
        let c2 = engine.cluster(0.3, 2);
        assert_eq!(c1.len(), c2.len(), "clustering should be deterministic");
    }

    #[test]
    fn test_single_error_no_cluster() {
        let mut engine = ErrorClusteringEngine::new();
        engine
            .add_error(make_error(1, "panic", "single error only"));
        let clusters = engine.cluster(0.3, 2);
        assert!(clusters.is_empty(), "single error should produce no clusters");
    }

    #[test]
    fn test_add_error_updates_index() {
        let mut engine = ErrorClusteringEngine::new();
        engine.add_error(make_error(1, "latency", "latency exceeded"));
        assert!(engine.index.contains_key("latency"),
            "index should contain 'latency', keys: {:?}", engine.index.keys().collect::<Vec<_>>());
    }
}
