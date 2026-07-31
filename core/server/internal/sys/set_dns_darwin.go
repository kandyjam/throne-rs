package sys

import (
	"ThroneCore/internal/boxdns"
	tun "github.com/sagernet/sing-tun"
	E "github.com/sagernet/sing/common/exceptions"
	"github.com/sagernet/sing/common/shell"
	"strings"
)

// SetSystemDNS points the physical default-route NIC at addr (or clears it when
// addr is "Empty"). Prefer the always-on boxdns monitor — it excludes TUN/loopback
// — so we never call networksetup against utun after auto_route flips the default
// route. Falling back to sing-box's monitor keeps prior behavior if boxdns is down.
func SetSystemDNS(addr string, interfaceMonitor tun.DefaultInterfaceMonitor) error {
	interfaceName := physicalInterfaceName(interfaceMonitor)
	if interfaceName == "" {
		return E.New("no physical default interface for system DNS")
	}
	interfaceDisplayName, err := getInterfaceDisplayName(interfaceName)
	if err != nil {
		return err
	}

	err = shell.Exec("/usr/sbin/networksetup", "-setdnsservers", interfaceDisplayName, addr).Attach().Run()
	if err != nil {
		return err
	}

	return nil
}

func physicalInterfaceName(interfaceMonitor tun.DefaultInterfaceMonitor) string {
	if ifc := boxdns.DefaultInterface(); ifc != nil && ifc.Name != "" {
		return ifc.Name
	}
	if interfaceMonitor != nil {
		if di := interfaceMonitor.DefaultInterface(); di != nil {
			return di.Name
		}
	}
	return ""
}

func getInterfaceDisplayName(name string) (string, error) {
	content, err := shell.Exec("/usr/sbin/networksetup", "-listallhardwareports").ReadOutput()
	if err != nil {
		return "", err
	}
	for _, deviceSpan := range strings.Split(string(content), "Ethernet Address") {
		if strings.Contains(deviceSpan, "Device: "+name) {
			substr := "Hardware Port: "
			deviceSpan = deviceSpan[strings.Index(deviceSpan, substr)+len(substr):]
			deviceSpan = deviceSpan[:strings.Index(deviceSpan, "\n")]
			return deviceSpan, nil
		}
	}
	return "", E.New(name, " not found in networksetup -listallhardwareports")
}
