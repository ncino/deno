use std::path::Path;



fn main() {
  let js_path = format!("{}/js", env!("CARGO_MANIFEST_DIR"));
  let js_path = Path::new(js_path.as_str());

  for entry in std::fs::read_dir(js_path).unwrap() {
    let entry = entry.unwrap();

    println!("cargo:rerun-if-changed={}", entry.path().to_str().unwrap());
  }
}
