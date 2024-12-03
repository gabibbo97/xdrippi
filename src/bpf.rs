use std::os::fd::AsRawFd;

use libbpf_rs::MapCore;

/// The BPF redirect manager is tasked with loading a BPF XDP program allowing the redirection of frames to userspace AF_XDP sockets.
pub struct BPFRedirectManager {
    bpf_object: libbpf_rs::Object,
    _bpf_link: libbpf_rs::Link,
}
impl BPFRedirectManager {

    /// Attach the XDP program to a given network interface
    pub fn attach(if_index: libc::c_uint) -> Self {
        // open object
        let bpf_object = libbpf_rs::ObjectBuilder::default()
            .open_memory(include_bytes!("../bpf/redirect.o")).unwrap()
            .load().unwrap();

        // attach
        let bpf_link = if let Some(prog) = bpf_object.progs_mut().find(|x| x.name() == "xdp_sock_redir") {
            prog.attach_xdp(if_index as _).unwrap()
        } else {
            panic!()
        };

        Self { bpf_object, _bpf_link: bpf_link }
    }

    /// Add an AF_XDP socket for all packets incoming from the NIC queue `queue_id`
    pub fn add_redirect(&mut self, queue_id: u32, socket_fd: impl AsRawFd) {
        if let Some(map) = self.bpf_object.maps_mut().find(|x| x.name() == "xsks_map") {
            map.update(&queue_id.to_ne_bytes(), &socket_fd.as_raw_fd().to_ne_bytes(), libbpf_rs::MapFlags::ANY).unwrap();
        }
    }

    /// Remove an AF_XDP socket for all packets incoming from the NIC queue `queue_id`
    pub fn del_redirect(&mut self, queue_id: u32) {
        if let Some(map) = self.bpf_object.maps_mut().find(|x| x.name() == "xsks_map") {
            map.delete(&(queue_id as i32).to_ne_bytes()).unwrap();
        }
    }

}
