/// Dalin L — 分代 GC 垃圾回收器
///
/// 三代分代 GC：
/// - gen-0 (年轻代)：新分配对象，频繁回收
/// - gen-1 (老年代)：存活超过一次回收的对象
/// - gen-2 (永久代)：永不回收的永久对象
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// GC 跟踪的对象引用
pub struct GcRoot {
    roots: RefCell<Vec<GcPtr>>,
}

impl Default for GcRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl GcRoot {
    pub fn new() -> Self {
        Self {
            roots: RefCell::new(Vec::new()),
        }
    }

    /// 注册一个 GC 根
    pub fn push(&self, ptr: GcPtr) {
        self.roots.borrow_mut().push(ptr);
    }

    /// 获取所有根引用
    pub fn all(&self) -> Vec<GcPtr> {
        self.roots.borrow().clone()
    }

    /// 清空根引用
    pub fn clear(&self) {
        self.roots.borrow_mut().clear();
    }
}

/// 堆对象指针（简化版：用 ID 模拟）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GcPtr {
    pub id: usize,
}

impl GcPtr {
    pub fn new(id: usize) -> Self {
        Self { id }
    }
}

/// 堆对象
#[derive(Debug, Clone)]
pub struct GcObject {
    pub id: usize,
    pub kind: String, // "int", "string", "array", "struct"
    pub refs: Vec<usize>, // 引用的其他对象 ID
    pub marked: bool,
}

impl GcObject {
    pub fn new(id: usize, kind: &str, refs: Vec<usize>) -> Self {
        Self {
            id,
            kind: kind.to_string(),
            refs,
            marked: false,
        }
    }
}

/// 分代 GC
pub struct GenerationalGC {
    // gen-0: 新分配对象 (年轻代)
    gen0: RefCell<HashMap<usize, GcObject>>,
    // gen-1: 存活超过一次 collection (老年代)
    gen1: RefCell<HashMap<usize, GcObject>>,
    // gen-2: 永久对象
    gen2: RefCell<HashSet<usize>>,
    // 根引用
    roots: RefCell<Vec<usize>>,
    next_id: RefCell<usize>,
    // gen-0 触发阈值
    threshold: usize,
}

impl Default for GenerationalGC {
    fn default() -> Self {
        Self::new()
    }
}

impl GenerationalGC {
    pub fn new() -> Self {
        Self {
            gen0: RefCell::new(HashMap::new()),
            gen1: RefCell::new(HashMap::new()),
            gen2: RefCell::new(HashSet::new()),
            roots: RefCell::new(Vec::new()),
            next_id: RefCell::new(1),
            threshold: 10,
        }
    }

    /// 设置 gen-0 触发 GC 的阈值
    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.threshold = threshold;
        self
    }

    /// 注册一个根对象 ID
    pub fn add_root(&self, id: usize) {
        self.roots.borrow_mut().push(id);
    }

    /// 清除所有根引用（GC 周期开始前调用）
    pub fn clear_roots(&self) {
        self.roots.borrow_mut().clear();
    }

    /// 获取当前根引用数量
    pub fn root_count(&self) -> usize {
        self.roots.borrow().len()
    }

    /// 将对象提升到 gen-2（永久代）
    pub fn pin_to_gen2(&self, id: usize) {
        self.gen2.borrow_mut().insert(id);
    }

    /// 检查 ID 是否在 gen-2 中
    pub fn is_in_gen2(&self, id: usize) -> bool {
        self.gen2.borrow().contains(&id)
    }

    /// 分配新对象，返回 GcPtr
    pub fn alloc(&self, kind: &str, refs: Vec<usize>) -> GcPtr {
        let mut id = self.next_id.borrow_mut();
        let obj_id = *id;
        *id += 1;

        let obj = GcObject::new(obj_id, kind, refs);
        self.gen0.borrow_mut().insert(obj_id, obj);
        GcPtr::new(obj_id)
    }

    /// 标记阶段（从 root set 出发）
    pub fn mark(&self) {
        // 清除所有对象的 marked 标记
        for (_, obj) in self.gen0.borrow_mut().iter_mut() {
            obj.marked = false;
        }
        for (_, obj) in self.gen1.borrow_mut().iter_mut() {
            obj.marked = false;
        }

        // 构建 root set：用户注册的根 + gen-2 对象
        let mut root_set = self.roots.borrow().clone();
        for id in self.gen2.borrow().iter() {
            root_set.push(*id);
        }

        // 从 root set 出发，DFS 标记所有可达对象
        let mut stack: Vec<usize> = root_set;
        let mut visited = HashSet::new();

        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }

            // 标记 gen-0 中的对象
            if let Some(obj) = self.gen0.borrow_mut().get_mut(&id) {
                obj.marked = true;
                for &ref_id in &obj.refs.clone() {
                    stack.push(ref_id);
                }
                continue;
            }

            // 标记 gen-1 中的对象
            if let Some(obj) = self.gen1.borrow_mut().get_mut(&id) {
                obj.marked = true;
                for &ref_id in &obj.refs.clone() {
                    stack.push(ref_id);
                }
            }
            // gen-2 中的对象始终可达，不需要标记
        }
    }

    /// 清扫阶段（收集未标记对象）
    /// 返回被回收的对象数量
    pub fn sweep(&self) -> usize {
        let mut collected = 0;

        // 收集 gen-0 中未标记的对象
        let mut gen0 = self.gen0.borrow_mut();
        gen0.retain(|_, obj| {
            if obj.marked {
                true
            } else {
                collected += 1;
                false
            }
        });

        // 收集 gen-1 中未标记的对象
        let mut gen1 = self.gen1.borrow_mut();
        gen1.retain(|_, obj| {
            if obj.marked {
                true
            } else {
                collected += 1;
                false
            }
        });

        collected
    }

    /// 提升 gen-0 到 gen-1
    pub fn promote(&self) {
        let mut gen0 = self.gen0.borrow_mut();
        let mut gen1 = self.gen1.borrow_mut();

        // 将 gen-0 中标记的对象移到 gen-1
        let to_promote: Vec<(usize, GcObject)> = gen0
            .iter()
            .filter(|(_, obj)| obj.marked)
            .map(|(id, obj)| (*id, obj.clone()))
            .collect();

        for (id, obj) in to_promote {
            gen1.insert(id, obj);
            gen0.remove(&id);
        }
    }

    /// 触发了就 GC
    /// 返回被回收的对象数量
    pub fn maybe_collect(&self) -> usize {
        if self.gen0.borrow().len() >= self.threshold {
            self.mark();
            // 提升存活对象到 gen-1
            self.promote();
            // 清扫未标记对象
            self.sweep()
        } else {
            0
        }
    }

    /// 强制执行一次完整 GC
    pub fn collect_full(&self) -> usize {
        self.mark();
        self.promote();
        self.sweep()
    }

    /// 获取 gen-0 中的对象数量
    pub fn gen0_count(&self) -> usize {
        self.gen0.borrow().len()
    }

    /// 获取 gen-1 中的对象数量
    pub fn gen1_count(&self) -> usize {
        self.gen1.borrow().len()
    }

    /// 获取 gen-2 中的对象数量
    pub fn gen2_count(&self) -> usize {
        self.gen2.borrow().len()
    }

    /// 获取 gen-0 中的对象（用于调试）
    pub fn gen0_objects(&self) -> Vec<GcObject> {
        self.gen0.borrow().values().cloned().collect()
    }

    /// 获取 gen-1 中的对象（用于调试）
    pub fn gen1_objects(&self) -> Vec<GcObject> {
        self.gen1.borrow().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gc_alloc() {
        let gc = GenerationalGC::new();
        let ptr = gc.alloc("int", vec![]);
        assert_eq!(ptr.id, 1);
        assert_eq!(gc.gen0_count(), 1);
        assert_eq!(gc.gen1_count(), 0);
        assert_eq!(gc.gen2_count(), 0);
    }

    #[test]
    fn test_gc_alloc_with_refs() {
        let gc = GenerationalGC::new();
        let parent = gc.alloc("struct", vec![]);
        let child = gc.alloc("int", vec![]);
        // 创建一个引用 child 的对象
        let _container = gc.alloc("struct", vec![parent.id, child.id]);
        assert_eq!(gc.gen0_count(), 3);
    }

    #[test]
    fn test_gc_collect_unreachable() {
        let gc = GenerationalGC::new();

        // 分配可达对象（注册为根）
        let reachable = gc.alloc("int", vec![]);
        gc.add_root(reachable.id);

        // 分配不可达对象（没有根引用）
        let _unreachable = gc.alloc("int", vec![]);

        assert_eq!(gc.gen0_count(), 2);

        // 执行 GC — 不可达对象应该被回收
        let collected = gc.collect_full();
        assert_eq!(collected, 1);
        // 可达对象被提升到 gen-1
        assert_eq!(gc.gen0_count(), 0);
        assert_eq!(gc.gen1_count(), 1);
    }

    #[test]
    fn test_gc_mark_sweep() {
        let gc = GenerationalGC::new();

        // 创建引用链：root -> mid -> leaf
        let leaf = gc.alloc("int", vec![]);
        let mid = gc.alloc("struct", vec![leaf.id]);
        let root = gc.alloc("struct", vec![mid.id]);
        gc.add_root(root.id);

        // 创建孤立对象（不可达）
        let _orphan = gc.alloc("string", vec![]);

        assert_eq!(gc.gen0_count(), 4);

        // 标记阶段：从 root 出发标记所有可达对象
        gc.mark();

        // 验证可达对象被标记
        let gen0_objs = gc.gen0_objects();
        let root_obj = gen0_objs.iter().find(|o| o.id == root.id).unwrap();
        assert!(root_obj.marked);
        let mid_obj = gen0_objs.iter().find(|o| o.id == mid.id).unwrap();
        assert!(mid_obj.marked);
        let leaf_obj = gen0_objs.iter().find(|o| o.id == leaf.id).unwrap();
        assert!(leaf_obj.marked);

        // 清扫阶段：回收未标记对象
        let collected = gc.sweep();
        assert_eq!(collected, 1); // 只有 orphan 被回收
        assert_eq!(gc.gen0_count(), 3);
    }

    #[test]
    fn test_gc_promotion() {
        let gc = GenerationalGC::new();

        // 分配对象并注册为根
        let ptr = gc.alloc("int", vec![]);
        gc.add_root(ptr.id);

        // 标记并提升
        gc.mark();
        gc.promote();

        // 验证：gen-0 中的对象被提升到 gen-1
        assert_eq!(gc.gen0_count(), 0);
        assert_eq!(gc.gen1_count(), 1);
    }

    #[test]
    fn test_gc_maybe_collect_below_threshold() {
        let gc = GenerationalGC::new().with_threshold(5);

        // 分配少于阈值的对象
        gc.alloc("int", vec![]);
        gc.alloc("int", vec![]);
        gc.alloc("int", vec![]);

        // 未达到阈值，不应触发 GC
        let collected = gc.maybe_collect();
        assert_eq!(collected, 0);
        assert_eq!(gc.gen0_count(), 3);
    }

    #[test]
    fn test_gc_maybe_collect_above_threshold() {
        let gc = GenerationalGC::new().with_threshold(3);

        // 分配 4 个对象（超过阈值 3）
        for _ in 0..4 {
            gc.alloc("int", vec![]);
        }
        gc.add_root(1); // 让第一个对象可达

        // 达到阈值，触发 GC
        let collected = gc.maybe_collect();
        // 3 个不可达对象被回收
        assert_eq!(collected, 3);
    }

    #[test]
    fn test_gc_pin_to_gen2() {
        let gc = GenerationalGC::new();

        let ptr = gc.alloc("int", vec![]);
        gc.pin_to_gen2(ptr.id);

        assert!(gc.is_in_gen2(ptr.id));
        assert_eq!(gc.gen2_count(), 1);

        // gen-2 对象在 GC 中应始终存活
        gc.collect_full();
        assert!(gc.is_in_gen2(ptr.id));
    }

    #[test]
    fn test_gc_cyclic_references() {
        let gc = GenerationalGC::new();

        // 创建两个互相引用的对象（循环引用），但不可达
        let a = gc.alloc("struct", vec![]);
        let b = gc.alloc("struct", vec![a.id]);
        // 更新 a 的 refs 以引用 b
        if let Some(obj) = gc.gen0.borrow_mut().get_mut(&a.id) {
            obj.refs.push(b.id);
        }

        // 不注册任何根，两个对象都应被回收
        let collected = gc.collect_full();
        assert_eq!(collected, 2);
        assert_eq!(gc.gen0_count(), 0);
    }

    #[test]
    fn test_gc_multi_generation_sweep() {
        let gc = GenerationalGC::new();

        // gen-0 中被标记的对象会被提升到 gen-1
        let reachable0 = gc.alloc("int", vec![]);
        gc.add_root(reachable0.id);
        let _unreachable0 = gc.alloc("int", vec![]);

        // 第一次 GC：reachable0 被提升到 gen-1，unreachable0 被回收
        gc.collect_full();
        assert_eq!(gc.gen0_count(), 0);
        assert_eq!(gc.gen1_count(), 1);

        // 在 gen-1 中再分配一个不可达对象
        let _unreachable1 = gc.alloc("int", vec![]);

        // 第二次 GC：gen-1 中的 reachable0 应该存活，gen-0 中的 unreachable1 被回收
        let collected = gc.collect_full();
        assert_eq!(collected, 1);
        assert_eq!(gc.gen1_count(), 1);
    }
}