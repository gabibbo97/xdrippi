# xdrippi

**xdrippi** is a Rust library focused on providing easy access to AF_XDP sockets and their goodies.

__At the moment, this library is very experimental__

## Getting started

### Requirements

This library requires:

- `clang` to build the BPF programs found inside of the `bpf` directory.
- `libbpf` to be able to install the BPF trampoline.

### Testing environment

On a Linux machine, run `make test-net` to assemble an 8-container configuration as follows:

|Container|IP Address|MAC Address|VETH name on host|
|---------|----------|-----------|-----------------|
|`test1`|`10.42.0.10`|`54:00:00:00:00:10`|`test1`|
|`test2`|`10.42.0.20`|`54:00:00:00:00:20`|`test2`|
|`test3`|`10.42.0.30`|`54:00:00:00:00:30`|`test3`|
|`test4`|`10.42.0.40`|`54:00:00:00:00:40`|`test4`|
|`test5`|`10.42.0.50`|`54:00:00:00:00:50`|`test5`|
|`test6`|`10.42.0.60`|`54:00:00:00:00:60`|`test6`|
|`test7`|`10.42.0.70`|`54:00:00:00:00:70`|`test7`|
|`test8`|`10.42.0.80`|`54:00:00:00:00:80`|`test8`|

This environment is expected by the following examples

#### Example 1: recv

```sh
cargo build --example recv && sudo ./target/debug/recv
```

This sample will simply receive and print packets received from the first container, to send a ping run `make test-ping-host`

#### Example 2: fwd

```sh
cargo build --example fwd && sudo ./target/debug/fwd
```

This sample forwards packets between container 1 and container 2 in a dumb way, i.e. everything flowing in on one side ends up on the other.

To enter the first container run `make shell-test1`.

To enter the second container run `make shell-test2`.

#### Example 3: l2

```sh
cargo build --example l2 && sudo ./target/debug/l2
```

This sample builds a very simple L2 switch with all 8 containers attached to it.

To enter any container run `make shell-test<x>` where `<x>` is `1`, ..., `8`.

## Licensing

GNU Affero General Public License version 3 or later.

Sorry, I have had my code stolen __with it__ and I wouldn't want to risk it again.

## Funding

The development of this library was indirectly sponsored by the [University of Genova](https://unige.it)

## Naming

Writing out the literal pronunciation of `XDP` in italian yields something like `xdippí`. Add in some brainrot about the `drip` meme and you can figure it out. 
