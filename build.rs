use prost_build::compile_protos;
use std::io::Result;

fn main() -> Result<()> {
    compile_protos(&["src/patch.proto"], &["src/"])?;
    Ok(())
}
