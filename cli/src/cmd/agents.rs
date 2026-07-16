/// Legacy run_agents_demo() — agent concurrency demo
use dalin_runtime::interpreter;

pub fn run() -> Result<(), String> {
    println!("============================================================");
    println!("  Dalin L 2.0 — Agent-Native 并发（spawn + channel）");
    println!("============================================================");

    let src = r#"
        channel tx rx
        spawn fn producer() @ spawn @ cpu {
            send(tx, 100)
            return 0
        }
        let got = recv(rx)
        println("父任务收到:", got)
        let status = await(producer)
        println("worker 退出状态:", status)
    "#;

    match interpreter::run_source(src) {
        Ok(_) => println!("\nAgent-Native 并发演示完成"),
        Err(e) => println!("\n{}", e),
    }

    Ok(())
}
