use std::io::Result;

use prost_build::compile_protos;

fn main() -> Result<()> {
    compile_protos(&["src/patch.proto"], &["src/"])?;
    Ok(())
}
