fn main() {
    // include_dir! embeds the corpus; YAML edits must invalidate the binary.
    println!("cargo:rerun-if-changed=data");
}
