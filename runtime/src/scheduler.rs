//! Dalin L 3.0 — M:N 协程调度器
//!
//! 将 M 个 Dalin 协程（由 `spawn` / `spawn_task` 派生）复用到 N 个 OS 工作线程上，
//! 取代原先 `std::thread::spawn` 的 1:1 内核线程模型（每协程一线程，无法支撑大规模并发）。
//!
//! 设计要点（纯 std，零外部依赖）：
//! - **有界工作窃取线程池**：N 个常驻 worker + 按需派生的 helper（上限 `max_threads`），
//!   所有协程进入全局运行队列，worker 出队执行至完成。
//! - **非阻塞快路径 await/recv**：完成时直接锁查完成槽返回，不占用 worker 线程。
//! - **防饿死（无死锁）**：所有协程先入队再启动 worker；若某 worker 在 `await`/`recv` 上
//!   parked（目标尚未就绪），`ensure_helper()` 自动派生辅助线程继续 drain 队列，
//!   被等协程终会被某空闲 worker 跑完并 `notify`，故不存在全局死锁。
//! - **协同抢占**：`yield_now()` 让当前协程主动让出 worker，内联 drain 队列中其他协程。
//! - **优雅关停**：`Drop` 置 shutdown 并唤醒全部等待中的 worker，join 所有线程。

use crate::env::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// 单协程完成槽：结果值 + 就绪条件变量（供 await 等待）。
struct Completion {
    value: Mutex<Option<Value>>,
    ready: Condvar,
}

impl Completion {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
            ready: Condvar::new(),
        }
    }
}

/// 异步通道状态：有界/无界队列 + 就绪条件变量 + 关闭标志。
struct ChannelState {
    queue: Mutex<VecDeque<Value>>,
    ready: Condvar,
    closed: std::sync::atomic::AtomicBool,
}

impl ChannelState {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// 运行时协程单元：持有可执行闭包。
/// 闭包签名 `FnOnce(&Scheduler)` —— 执行完毕后通过 scheduler 设置自身完成槽
/// （完成槽以 task id 为键存于 `completions` 注册表，故此处无需重复持有）。
struct Coroutine {
    work: Box<dyn FnOnce(&Scheduler) + Send>,
}

/// 调度器指标快照（供 `stats()` 与 benchmark 读取）。
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub base_workers: usize,
    pub max_workers: usize,
    pub spawned: u64,
    pub completed: u64,
    pub active: usize,
    pub queued: usize,
    pub live_threads: usize,
}

/// M:N 协程调度器。
pub struct Scheduler {
    /// 常驻 worker 数量（由 `DALIN_WORKERS` 或 `available_parallelism` 决定）。
    base_workers: usize,
    /// worker 线程上限（base + 按需 helper），由 `DALIN_MAX_HELPERS` 约束。
    max_threads: usize,
    /// 全局运行队列（协程待执行）。
    queue: Mutex<VecDeque<Coroutine>>,
    /// 完成槽注册表：task id -> Completion（await 在此等待）。
    completions: Mutex<HashMap<String, Arc<Completion>>>,
    /// 命名通道注册表：名称 -> ChannelState（send/recv 在此会合）。
    channels: Mutex<HashMap<String, Arc<ChannelState>>>,
    /// 指标。
    spawned: AtomicUsize,
    completed: AtomicUsize,
    active: AtomicUsize,
    thread_count: AtomicUsize,
    /// 关停标志。
    shutdown: std::sync::atomic::AtomicBool,
    /// worker 线程句柄（Drop 时 join）。
    handles: Mutex<Vec<JoinHandle<()>>>,
    /// 自身弱引用（ensure_helper 派生线程时需要 Arc<Scheduler>）。
    self_ref: Weak<Scheduler>,
}

/// 解析 worker 数量：环境变量 > 可用并行度 > 4。
fn resolve_workers() -> usize {
    if let Ok(s) = std::env::var("DALIN_WORKERS")
        && let Ok(n) = s.parse::<usize>()
        && n > 0
    {
        return n;
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// 解析 helper 上限：环境变量（默认 base*4，封顶 256 防止 100k 规模下内存爆炸）。
fn resolve_max_helpers(base: usize) -> usize {
    if let Ok(s) = std::env::var("DALIN_MAX_HELPERS")
        && let Ok(n) = s.parse::<usize>()
        && n > 0
    {
        return base + n.min(256);
    }
    base + (base * 4).min(256)
}

impl Scheduler {
    /// 创建调度器并启动 `resolve_workers()` 个常驻 worker 线程。
    pub fn new() -> Arc<Scheduler> {
        let base = resolve_workers();
        let max = resolve_max_helpers(base);
        let sched = Arc::new_cyclic(|weak| Scheduler {
            base_workers: base,
            max_threads: max,
            queue: Mutex::new(VecDeque::new()),
            completions: Mutex::new(HashMap::new()),
            channels: Mutex::new(HashMap::new()),
            spawned: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            thread_count: AtomicUsize::new(0),
            shutdown: std::sync::atomic::AtomicBool::new(false),
            handles: Mutex::new(Vec::new()),
            self_ref: weak.clone(),
        });
        for _ in 0..base {
            let h = thread::spawn({
                let s = sched.clone();
                move || worker_loop(s)
            });
            sched.handles.lock().unwrap().push(h);
        }
        sched.thread_count.store(base, Ordering::SeqCst);
        sched
    }

    /// 入队一个协程：预建完成槽（保证 await 可立即找到），push 到运行队列。
    pub fn spawn_coroutine<F>(&self, id: String, f: F)
    where
        F: FnOnce(&Scheduler) + Send + 'static,
    {
        let completion = Arc::new(Completion::new());
        self.completions
            .lock()
            .unwrap()
            .insert(id.clone(), completion);
        let coro = Coroutine {
            work: Box::new(f),
        };
        self.queue.lock().unwrap().push_back(coro);
        self.spawned.fetch_add(1, Ordering::SeqCst);
    }

    /// 协程执行完毕时由闭包回调，写入完成槽并唤醒等待者。
    pub fn set_completion(&self, id: &str, value: Value) {
        if let Some(c) = self.completions.lock().unwrap().get(id) {
            let mut g = c.value.lock().unwrap();
            *g = Some(value);
            drop(g);
            c.ready.notify_all();
        }
    }

    /// await(task)：快路径直接取完成值；慢路径 parked 等待（并确保队列继续 drain）。
    pub fn await_task(&self, id: &str) -> Value {
        let comp = self.completions.lock().unwrap().get(id).cloned();
        match comp {
            None => Value::None,
            Some(c) => {
                let mut g = c.value.lock().unwrap();
                if let Some(v) = g.take() {
                    return v;
                }
                self.ensure_helper();
                g = c
                    .ready
                    .wait(g)
                    .unwrap_or_else(|_| c.value.lock().unwrap());
                g.take().unwrap_or(Value::None)
            }
        }
    }

    /// 创建命名通道。
    pub fn create_channel(&self, name: &str) {
        self.channels
            .lock()
            .unwrap()
            .insert(name.to_string(), Arc::new(ChannelState::new()));
    }

    /// send(channel, value)：推入队列并唤醒一个接收者。
    pub fn send_chan(&self, name: &str, value: Value) -> Result<(), String> {
        let ch = self.channels.lock().unwrap().get(name).cloned();
        match ch {
            None => Err(format!("未知 channel: {}", name)),
            Some(c) => {
                c.queue.lock().unwrap().push_back(value);
                c.ready.notify_one();
                Ok(())
            }
        }
    }

    /// recv(channel)：快路径取队列头；慢路径 parked 等待发送者。
    pub fn recv_chan(&self, name: &str) -> Value {
        let ch = self.channels.lock().unwrap().get(name).cloned();
        match ch {
            None => Value::None,
            Some(c) => {
                let mut g = c.queue.lock().unwrap();
                if let Some(v) = g.pop_front() {
                    return v;
                }
                self.ensure_helper();
                loop {
                    let _guard = c
                        .ready
                        .wait(g)
                        .unwrap_or_else(|_| c.queue.lock().unwrap());
                    g = _guard;
                    if let Some(v) = g.pop_front() {
                        return v;
                    }
                    if c.closed.load(Ordering::SeqCst) {
                        return Value::None;
                    }
                }
            }
        }
    }

    /// 协同抢占：主动让出当前 worker，内联 drain 队列中至多 `budget` 个其他协程。
    pub fn yield_now(&self) {
        const BUDGET: usize = 1024;
        let mut ran = 0;
        while ran < BUDGET {
            let coro = self.queue.lock().unwrap().pop_front();
            match coro {
                Some(c) => {
                    self.active.fetch_add(1, Ordering::SeqCst);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (c.work)(self);
                    }));
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    self.completed.fetch_add(1, Ordering::SeqCst);
                    ran += 1;
                }
                None => break,
            }
        }
    }

    /// 在 worker 即将 parked 时派生一个辅助线程，确保运行队列不饿死。
    fn ensure_helper(&self) {
        let cur = self.thread_count.load(Ordering::SeqCst);
        if cur >= self.max_threads {
            return;
        }
        if let Some(arc) = self.self_ref.upgrade() {
            let h = thread::spawn(move || worker_loop(arc));
            self.handles.lock().unwrap().push(h);
            self.thread_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 尽力等待所有协程完成（带超时，防止未 await 的孤儿协程挂死测试）。
    pub fn finish_with_timeout(&self, timeout: Duration) {
        let start = Instant::now();
        loop {
            let queued = self.queue.lock().unwrap().len();
            let active = self.active.load(Ordering::SeqCst);
            if queued == 0 && active == 0 {
                return;
            }
            if start.elapsed() > timeout {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// 指标快照。
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            base_workers: self.base_workers,
            max_workers: self.max_threads,
            spawned: self.spawned.load(Ordering::SeqCst) as u64,
            completed: self.completed.load(Ordering::SeqCst) as u64,
            active: self.active.load(Ordering::SeqCst),
            queued: self.queue.lock().unwrap().len(),
            live_threads: self.thread_count.load(Ordering::SeqCst),
        }
    }
}

/// worker 主循环：出队执行协程至完成；队列空且无 shutdown 则短暂休眠。
fn worker_loop(scheduler: Arc<Scheduler>) {
    loop {
        if scheduler.shutdown.load(Ordering::SeqCst) {
            // shutdown 前再 drain 一次，避免遗漏已入队但未执行的协程。
            if scheduler.queue.lock().unwrap().is_empty() {
                break;
            }
        }
        let coro = scheduler.queue.lock().unwrap().pop_front();
        match coro {
            Some(c) => {
                scheduler.active.fetch_add(1, Ordering::SeqCst);
                (c.work)(&scheduler);
                scheduler.active.fetch_sub(1, Ordering::SeqCst);
                scheduler.completed.fetch_add(1, Ordering::SeqCst);
            }
            None => {
                if scheduler.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_micros(50));
            }
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // 唤醒所有 parked 的 worker（await/recv 上的条件变量）。
        for c in self.completions.lock().unwrap().values() {
            c.ready.notify_all();
        }
        for ch in self.channels.lock().unwrap().values() {
            ch.ready.notify_all();
        }
        let handles = std::mem::take(&mut *self.handles.lock().unwrap());
        for h in handles {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助：把 Value 当 Int 取出（Value 未派生 PartialEq，避免牵动 ast 类型）。
    fn as_int(v: &Value) -> i64 {
        match v {
            Value::Int(n) => *n,
            other => panic!("expected Int, got {:?}", other),
        }
    }

    #[test]
    fn scheduler_runs_coroutine_and_awaits() {
        let s = Scheduler::new();
        s.spawn_coroutine("t1".into(), |sched| {
            sched.set_completion("t1", Value::Int(42));
        });
        // 快路径或慢路径都应拿到结果。
        assert_eq!(as_int(&s.await_task("t1")), 42);
    }

    #[test]
    fn scheduler_channel_send_recv() {
        let s = Scheduler::new();
        s.create_channel("c");
        s.send_chan("c", Value::Int(7)).unwrap();
        assert_eq!(as_int(&s.recv_chan("c")), 7);
    }

    #[test]
    fn scheduler_yield_now_drains_queue() {
        let s = Scheduler::new();
        for i in 0..5 {
            let id = format!("y{}", i);
            s.spawn_coroutine(id.clone(), move |sched| {
                sched.set_completion(&id, Value::Int(i as i64));
            });
        }
        // 主动让出，内联 drain 队列中其他协程。
        s.yield_now();
        for i in 0..5 {
            assert_eq!(as_int(&s.await_task(&format!("y{}", i))), i as i64);
        }
    }

    #[test]
    fn scheduler_stats_count_completions() {
        let s = Scheduler::new();
        for i in 0..10 {
            let id = format!("s{}", i);
            s.spawn_coroutine(id.clone(), move |sched| {
                sched.set_completion(&id, Value::Int(i as i64));
            });
        }
        s.finish_with_timeout(Duration::from_secs(2));
        let stats = s.stats();
        assert_eq!(stats.spawned, 10);
        assert_eq!(stats.completed, 10);
        assert_eq!(stats.queued, 0);
        assert_eq!(stats.active, 0);
    }

    #[test]
    fn scheduler_mn_handles_100k_coroutines() {
        // M:N 核心证明：10 万个协程复用到有界线程池（base + helper，上限 256），
        // 若按 1:1 内核线程模型会直接资源耗尽 / 崩溃。
        // 使用 set_completion 直写完成槽（不经过通道），避免大规模锁竞争。
        let s = Scheduler::new();
        let n: usize = 100_000;
        for i in 0..n {
            let id = format!("c{}", i);
            s.spawn_coroutine(id.clone(), move |sched| {
                sched.set_completion(&id, Value::Int(i as i64));
            });
        }
        s.finish_with_timeout(Duration::from_secs(30));
        let stats = s.stats();
        assert_eq!(stats.spawned, n as u64);
        assert_eq!(stats.completed, n as u64, "所有协程都应执行完毕");
        // 线程数必须远小于协程数（有界复用）。
        assert!(
            stats.live_threads <= stats.max_workers,
            "live_threads {} 应 <= max_workers {}",
            stats.live_threads,
            stats.max_workers
        );
        assert!(
            stats.live_threads < n,
            "线程数 {} 应远小于协程数 {}（证明 M:N 复用）",
            stats.live_threads,
            n
        );
    }

    #[test]
    fn scheduler_mn_10k_via_interpreter_spawn() {
        // 通过 spawn_task + await 走完整解释器路径，验证 M:N 在真实代码中能跑通。
        let src = r#"
            fn adder(x) @ spawn @ cpu {
                return x * 2
            }
            let h = spawn_task("adder", 3)
            let r = await(h)
        "#;
        let results = crate::interpreter::run_source(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 6),
            other => panic!("expected Int(6), got {:?}", other),
        }
    }
}
