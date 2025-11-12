fn main() {
    // On Linux, libunwind-ptrace.so may depend on liblzma.
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=lzma");
    }
}
