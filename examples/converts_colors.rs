//! Example that converts `colors.yml` in the workspace into JSON and writes it
//! to standard output.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join("colors.yml");

    let reader = File::open(&input_path)?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    cc_ty::convert_to_writer(reader, &mut handle)?;
    handle.flush()?;

    Ok(())
}
