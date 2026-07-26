fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tonic-build 0.14: compile_protos moved to tonic-prost-build
    tonic_prost_build::compile_protos("proto/control.proto")?;
    Ok(())
}
