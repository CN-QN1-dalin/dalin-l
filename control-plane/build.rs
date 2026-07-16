fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 用系统 protoc 编译 proto（本机已装 libprotoc 34.1）。
    // 若 CI 无 protoc，可改走 protoc-bin-vendored，本地不需要。
    tonic_build::compile_protos("proto/control.proto")?;
    Ok(())
}
