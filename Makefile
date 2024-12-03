CONTAINERS := 1 2 3 4 5 6 7 8

.PHONY: test-container-%
test-container-%:
# extract name
	$(eval I := $(subst test-container-,,$@))
# cleanup
	sudo ip netns del test$(I) 2>/dev/null || true
	sudo ip link del test$(I) 2>/dev/null || true
# network setup
	sudo ip netns add test$(I)
	sudo ip netns exec test$(I) ip link set dev lo up
	sudo ip netns exec test$(I) ip link add eth0 type veth peer name test$(I)
	sudo ip netns exec test$(I) ip link set dev test$(I) netns 1
	sudo ip netns exec test$(I) ip link set dev eth0 address 54:00:00:00:00:$(I)0
	sudo ip netns exec test$(I) ip addr add 10.42.0.$(I)0/24 dev eth0
	sudo ip netns exec test$(I) ip link set dev eth0 up
	sudo ip netns exec test$(I) ip neigh add 10.42.0.$(I) lladdr 54:00:00:00:00:0$(I) nud permanent dev eth0

	sudo ip link set dev test$(I) up
	sudo ip link set dev test$(I) address 54:00:00:00:00:0$(I)
	sudo ip addr add 10.42.0.$(I)/32 dev test$(I) noprefixroute
	sudo ip route add to 10.42.0.$(I)0/32 dev test$(I) src 10.42.0.$(I) 
# disable TX hardware offload
	sudo ethtool -K test$(I) tx off
	sudo ip netns exec test$(I) ethtool -K eth0 tx off

.PHONY: test-net
test-net: $(foreach container,$(CONTAINERS),test-container-$(container) )

.PHONY: test-cross-ping
test-cross-ping:
	sudo ip netns exec test1 ping 10.42.0.20

.PHONY: test-ping-host
test-ping-host:
	timeout 1 sudo ip netns exec test1 ping -c1 -w5 10.42.0.1

.PHONY: test-flood-ping-host
test-flood-ping-host:
	sudo ip netns exec test1 ping -f 10.42.0.1

.PHONY: shell-%
shell-%:
	$(eval NAME := $(subst shell-,,$@))
	sudo ip netns exec $(NAME) bash
