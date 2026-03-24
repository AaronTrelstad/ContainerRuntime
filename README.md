# ContainerRuntime

This project involves the development of a minimal, OCI-compatible container runtime
implemented in Rust to explore the security primitives of cloud-native isolation. The system will
implement core Linux kernel features including namespaces for process isolation, cgroup for
resource enforcement, and seccomp filtering to restrict system call access. By adhering to the
OCI Runtime Specification, the software will provide a standardized environment for executing
"filesystem bundles", effectively hiding interaction details from the end user while maintaining
robust security.
The runtime will be deployed and evaluated on AWS EC2 to analyze its performance and
security strength within a production cloud environment. The final report will include a detailed
architectural breakdown using diagrams to illustrate the isolation layers. This work aligns with
course goals by exploring how system-level security mechanisms can be applied to protect
cloud-based deployments and users' privacy.

# Usage
cargo build
cargo run
