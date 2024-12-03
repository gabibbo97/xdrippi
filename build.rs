fn main() {
    println!("cargo::rerun-if-changed=bpf/redirect.c");
    if ! std::process::Command::new("clang")
        .arg("-O2").arg("-g")
        .arg("-target").arg("bpf")
        .arg("-c").arg("bpf/redirect.c")
        .arg("-o").arg(format!("{}/redirect.o", std::env::var("OUT_DIR").unwrap()))
        .status().unwrap()
        .success() {
            panic!("Failed compiling BPF program")
        }
}
