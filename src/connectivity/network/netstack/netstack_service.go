// Copyright 2017 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// +build !build_with_native_toolchain

package netstack

import (
	"context"
	"errors"
	"net"
	"sort"
	"syscall/zx"
	"syscall/zx/fidl"
	"syscall/zx/zxwait"

	"go.fuchsia.dev/fuchsia/src/connectivity/network/netstack/fidlconv"
	"go.fuchsia.dev/fuchsia/src/connectivity/network/netstack/link/eth"
	"go.fuchsia.dev/fuchsia/src/connectivity/network/netstack/routes"
	"go.fuchsia.dev/fuchsia/src/lib/component"
	syslog "go.fuchsia.dev/fuchsia/src/lib/syslog/go"

	"fidl/fuchsia/hardware/ethernet"
	fidlnet "fidl/fuchsia/net"
	"fidl/fuchsia/net/dhcp"
	"fidl/fuchsia/netstack"

	"gvisor.dev/gvisor/pkg/tcpip"
	"gvisor.dev/gvisor/pkg/tcpip/header"
	"gvisor.dev/gvisor/pkg/tcpip/network/ipv4"
	"gvisor.dev/gvisor/pkg/tcpip/network/ipv6"
	tcpipstack "gvisor.dev/gvisor/pkg/tcpip/stack"
)

type netstackImpl struct {
	ns *Netstack
}

// GetInterfaces returns a list of interfaces.
func (ni *netstackImpl) GetInterfaces(fidl.Context) ([]netstack.NetInterface, error) {
	nicInfos := ni.ns.stack.NICInfo()
	interfaces := make([]netstack.NetInterface, 0, len(nicInfos))
	for _, nicInfo := range nicInfos {
		ifs := nicInfo.Context.(*ifState)

		var ipv6addrs []fidlnet.Subnet
		for _, address := range nicInfo.ProtocolAddresses {
			if address.Protocol == ipv6.ProtocolNumber {
				ipv6addrs = append(ipv6addrs, fidlnet.Subnet{
					Addr:      fidlconv.ToNetIpAddress(address.AddressWithPrefix.Address),
					PrefixLen: uint8(address.AddressWithPrefix.PrefixLen),
				})
			}
		}

		var flags netstack.Flags
		ifs.mu.Lock()
		if ifs.IsUpLocked() {
			flags |= netstack.FlagsUp
		}
		if ifs.mu.dhcp.enabled {
			flags |= netstack.FlagsDhcp
		}
		metric := uint32(ifs.mu.metric)
		ifs.mu.Unlock()

		netInterface := netstack.NetInterface{
			Id:        uint32(ifs.nicid),
			Flags:     flags,
			Metric:    metric,
			Name:      nicInfo.Name,
			Hwaddr:    []uint8(ifs.endpoint.LinkAddress()[:]),
			Ipv6addrs: ipv6addrs,
		}

		if ifs.endpoint.Capabilities()&tcpipstack.CapabilityLoopback != 0 {
			netInterface.Features |= ethernet.FeaturesLoopback
		}

		if client, ok := ifs.controller.(*eth.Client); ok {
			netInterface.Features |= client.Info.Features
		}

		addrWithPrefix, err := ifs.ns.stack.GetMainNICAddress(ifs.nicid, ipv4.ProtocolNumber)
		if err != nil {
			_ = syslog.Warnf("stack.GetMainNICAddress(%d, ipv4.ProtocolNumber): %s", ifs.nicid, err)
		}

		if addrWithPrefix == (tcpip.AddressWithPrefix{}) {
			addrWithPrefix = tcpip.AddressWithPrefix{Address: header.IPv4Any, PrefixLen: 0}
		}
		mask := net.CIDRMask(addrWithPrefix.PrefixLen, len(addrWithPrefix.Address)*8)
		broadaddr := []byte(addrWithPrefix.Address)
		for i := range broadaddr {
			broadaddr[i] |= ^mask[i]
		}
		netInterface.Addr = fidlconv.ToNetIpAddress(addrWithPrefix.Address)
		netInterface.Netmask = fidlconv.ToNetIpAddress(tcpip.Address(mask))
		netInterface.Broadaddr = fidlconv.ToNetIpAddress(tcpip.Address(broadaddr))

		interfaces = append(interfaces, netInterface)
	}

	sort.Slice(interfaces, func(i, j int) bool {
		return interfaces[i].Id < interfaces[j].Id
	})

	return interfaces, nil
}

func (ni *netstackImpl) GetRouteTable(fidl.Context) ([]netstack.RouteTableEntry, error) {
	table := ni.ns.GetExtendedRouteTable()
	out := make([]netstack.RouteTableEntry, 0, len(table))
	for _, e := range table {
		var gatewayPtr *fidlnet.IpAddress
		if len(e.Route.Gateway) != 0 {
			gateway := fidlconv.ToNetIpAddress(e.Route.Gateway)
			gatewayPtr = &gateway
		}
		out = append(out, netstack.RouteTableEntry{
			Destination: fidlconv.ToNetIpAddress(e.Route.Destination.ID()),
			Netmask:     fidlconv.ToNetIpAddress(tcpip.Address(e.Route.Destination.Mask())),
			Gateway:     gatewayPtr,
			Nicid:       uint32(e.Route.NIC),
			Metric:      uint32(e.Metric),
		})
	}
	return out, nil
}

func routeToNs(r netstack.RouteTableEntry) tcpip.Route {
	prefixLen, _ := net.IPMask(fidlconv.ToTCPIPAddress(r.Netmask)).Size()
	route := tcpip.Route{
		Destination: fidlconv.ToTCPIPSubnet(fidlnet.Subnet{
			Addr:      r.Destination,
			PrefixLen: uint8(prefixLen),
		}),
		NIC: tcpip.NICID(r.Nicid),
	}
	if g := r.Gateway; g != nil {
		route.Gateway = fidlconv.ToTCPIPAddress(*g)
	}
	return route
}

type routeTableTransactionImpl struct {
	ni *netstackImpl
}

func (i *routeTableTransactionImpl) AddRoute(_ fidl.Context, r netstack.RouteTableEntry) (int32, error) {
	err := i.ni.ns.AddRoute(routeToNs(r), routes.Metric(r.Metric), false /* not dynamic */)
	if err != nil {
		return int32(zx.ErrInvalidArgs), err
	}
	return int32(zx.ErrOk), nil
}

func (i *routeTableTransactionImpl) DelRoute(_ fidl.Context, r netstack.RouteTableEntry) (int32, error) {
	err := i.ni.ns.DelRoute(routeToNs(r))
	if err != nil {
		return int32(zx.ErrInvalidArgs), err
	}
	return int32(zx.ErrOk), nil
}

func (ni *netstackImpl) StartRouteTableTransaction(_ fidl.Context, req netstack.RouteTableTransactionWithCtxInterfaceRequest) (int32, error) {
	{
		ni.ns.mu.Lock()
		defer ni.ns.mu.Unlock()

		if ni.ns.mu.transactionRequest != nil {
			oldChannel := ni.ns.mu.transactionRequest.Channel
			observed, _ := zxwait.Wait(zx.Handle(oldChannel), 0, 0)
			// If the channel is neither readable nor writable, there is no
			// data left to be processed (not readable) and we can't return
			// any more results (not writable).  It's not enough to only
			// look at peerclosed because the peer can close the channel
			// while it still has data in its buffers.
			if observed&(zx.SignalChannelReadable|zx.SignalChannelWritable) == 0 {
				ni.ns.mu.transactionRequest = nil
			}
		}
		if ni.ns.mu.transactionRequest != nil {
			return int32(zx.ErrShouldWait), nil
		}
		ni.ns.mu.transactionRequest = &req
	}
	stub := netstack.RouteTableTransactionWithCtxStub{Impl: &routeTableTransactionImpl{ni: ni}}
	go component.ServeExclusive(context.Background(), &stub, req.Channel, func(err error) {
		// NB: this protocol is not discoverable, so the bindings do not include its name.
		_ = syslog.WarnTf("fuchsia.netstack.RouteTableTransaction", "%s", err)
	})
	return int32(zx.ErrOk), nil
}

// Add address to the given network interface.
func (ni *netstackImpl) SetInterfaceAddress(_ fidl.Context, nicid uint32, address fidlnet.IpAddress, prefixLen uint8) (netstack.NetErr, error) {
	protocolAddr := toProtocolAddr(fidlnet.Subnet{
		Addr:      address,
		PrefixLen: prefixLen,
	})
	if protocolAddr.AddressWithPrefix.PrefixLen > 8*len(protocolAddr.AddressWithPrefix.Address) {
		return netstack.NetErr{Status: netstack.StatusParseError, Message: "prefix length exceeds address length"}, nil
	}

	found, err := ni.ns.addInterfaceAddress(tcpip.NICID(nicid), protocolAddr)
	if err != nil {
		return netstack.NetErr{Status: netstack.StatusUnknownError, Message: err.Error()}, nil
	}
	if !found {
		return netstack.NetErr{Status: netstack.StatusUnknownInterface}, nil
	}
	return netstack.NetErr{Status: netstack.StatusOk}, nil
}

func (ni *netstackImpl) RemoveInterfaceAddress(_ fidl.Context, nicid uint32, address fidlnet.IpAddress, prefixLen uint8) (netstack.NetErr, error) {
	protocolAddr := toProtocolAddr(fidlnet.Subnet{
		Addr:      address,
		PrefixLen: prefixLen,
	})
	if protocolAddr.AddressWithPrefix.PrefixLen > 8*len(protocolAddr.AddressWithPrefix.Address) {
		return netstack.NetErr{Status: netstack.StatusParseError, Message: "prefix length exceeds address length"}, nil
	}

	found, err := ni.ns.removeInterfaceAddress(tcpip.NICID(nicid), protocolAddr)
	if err != nil {
		return netstack.NetErr{Status: netstack.StatusUnknownError, Message: err.Error()}, nil
	}
	if !found {
		return netstack.NetErr{Status: netstack.StatusUnknownInterface}, nil
	}
	return netstack.NetErr{Status: netstack.StatusOk}, nil
}

// SetInterfaceMetric changes the metric for an interface and updates all
// routes tracking that interface metric. This takes the lock.
func (ni *netstackImpl) SetInterfaceMetric(_ fidl.Context, nicid uint32, metric uint32) (result netstack.NetErr, err error) {
	_ = syslog.Infof("update interface metric for NIC %d to metric=%d", nicid, metric)

	nic := tcpip.NICID(nicid)
	m := routes.Metric(metric)

	nicInfo, ok := ni.ns.stack.NICInfo()[nic]
	if !ok {
		return netstack.NetErr{Status: netstack.StatusUnknownInterface}, nil
	}

	ifState := nicInfo.Context.(*ifState)
	ifState.updateMetric(m)

	ni.ns.routeTable.UpdateMetricByInterface(nic, m)
	ni.ns.routeTable.UpdateStack(ni.ns.stack)
	return netstack.NetErr{Status: netstack.StatusOk}, nil
}

func (ni *netstackImpl) BridgeInterfaces(_ fidl.Context, nicids []uint32) (netstack.NetErr, uint32, error) {
	nics := make([]tcpip.NICID, len(nicids))
	for i, n := range nicids {
		nics[i] = tcpip.NICID(n)
	}
	ifs, err := ni.ns.Bridge(nics)
	if err != nil {
		return netstack.NetErr{Status: netstack.StatusUnknownError, Message: err.Error()}, 0, nil
	}
	return netstack.NetErr{Status: netstack.StatusOk}, uint32(ifs.nicid), nil
}

func (ni *netstackImpl) SetInterfaceStatus(_ fidl.Context, nicid uint32, enabled bool) error {
	if nicInfo, ok := ni.ns.stack.NICInfo()[tcpip.NICID(nicid)]; ok {
		if err := nicInfo.Context.(*ifState).setState(enabled); err != nil {
			_ = syslog.Errorf("(NIC %d).setState(enabled=%t): %s", nicid, enabled, err)
		}
	} else {
		_ = syslog.Warnf("(NIC %d).setState(enabled=%t): not found", nicid, enabled)
	}
	return nil
}

func (ni *netstackImpl) GetDhcpClient(_ fidl.Context, id uint32, request dhcp.ClientWithCtxInterfaceRequest) (netstack.NetstackGetDhcpClientResult, error) {
	var result netstack.NetstackGetDhcpClientResult
	nicid := tcpip.NICID(id)
	if _, ok := ni.ns.stack.NICInfo()[nicid]; !ok {
		result.SetErr(int32(zx.ErrNotFound))
		_ = request.Close()
		return result, nil
	}
	stub := dhcp.ClientWithCtxStub{Impl: &clientImpl{ns: ni.ns, nicid: nicid}}
	go component.ServeExclusive(context.Background(), &stub, request.Channel, func(err error) {
		// NB: this protocol is not discoverable, so the bindings do not include its name.
		_ = syslog.WarnTf("fuchsia.net.dhcp.Client", "%s", err)
	})
	result.SetResponse(netstack.NetstackGetDhcpClientResponse{})
	return result, nil
}

func (ni *netstackImpl) AddEthernetDevice(_ fidl.Context, topopath string, interfaceConfig netstack.InterfaceConfig, device ethernet.DeviceWithCtxInterface) (netstack.NetstackAddEthernetDeviceResult, error) {
	var result netstack.NetstackAddEthernetDeviceResult
	if ifs, err := ni.ns.addEth(topopath, interfaceConfig, &device); err != nil {
		var tcpipErr *TcpIpError
		if errors.As(err, &tcpipErr) {
			result.SetErr(int32(tcpipErr.ToZxStatus()))
		} else {
			result.SetErr(int32(zx.ErrInternal))
		}
	} else {
		result.SetResponse(netstack.NetstackAddEthernetDeviceResponse{Nicid: uint32(ifs.nicid)})
	}
	return result, nil
}
