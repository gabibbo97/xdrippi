#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

struct {
    __uint(type, BPF_MAP_TYPE_XSKMAP);
    __type(key, __u32);
    __type(value, __u32);
    __uint(max_entries, 64);
} xsks_map SEC(".maps");

SEC("xdp")
int xdp_sock_redir(struct xdp_md *ctx)
{
    // we will redirect according to the queue id
    __u32 queue_id = ctx->rx_queue_index;

    // lookup and send
    return bpf_redirect_map(&xsks_map, queue_id, XDP_DROP);
}

char _license[] SEC("license") = "GPL";
