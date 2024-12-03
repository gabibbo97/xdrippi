#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Memory allocation failure")] MemoryAllocationFailure,
    #[error("Memory map failure")] MemoryMapFailure,
    #[error("Poll failure")] PollFailure,
    #[error("Socket bind failure")] SocketBindFailure { error: std::io::Error },
    #[error("Socket creation failure")] SocketCreationFailure,
    #[error("Socket getsockopt failure (error = {error}, level = {level}, name = {name})")] SocketGetOptionFailure { error: std::io::Error, level: libc::c_int, name: libc::c_int },
    #[error("Socket getsockopt failure (expecting size {expecting} received size {received})")] SocketGetOptionSizeFailure { expecting: usize, received: usize },
    #[error("Socket send failure (error = {error})")] SocketSendFailure { error: std::io::Error },
    #[error("Socket setsockopt failure (error = {error}, level = {level}, name = {name})")] SocketSetOptionFailure { error: std::io::Error, level: libc::c_int, name: libc::c_int },
}