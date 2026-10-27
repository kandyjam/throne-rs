package sysdns

import (
	"errors"
	"os"
	"slices"
)

// DNSOverride supplies the original DNS while the system points to our loopback listener.
// Its structural interface is also consumed by the pinned Darwin systemconfig patch.
type DNSOverride struct{}

// A nonempty alias identifies the address to exclude. The bool reports whether
// the saved servers are known: only a known empty list permits DHCP fallback.
// Unknown ownership must exclude the alias without guessing the original DNS.
func (DNSOverride) SystemDNSOverride(device string) (string, []string, bool) {
	mu.Lock()
	defer mu.Unlock()
	return systemDNSOverride(device, owned, loadRecord, alive)
}

func systemDNSOverride(device string, local record, load func() (record, error), isAlive func(int) bool) (string, []string, bool) {
	if servers, known := savedServers(local, device); known {
		return Address, servers, true
	}
	// Probe cores can run separately from the owner. A permission error on the
	// root-only record means unknown DNS, not an unmanaged resolver. Never use
	// stale data, infer DHCP from inaccessible state, or relax record permissions.
	stored, err := load()
	if errors.Is(err, os.ErrNotExist) && local.PID == 0 {
		return "", nil, false
	}
	if err == nil && isAlive(stored.PID) {
		if servers, known := savedServers(stored, device); known {
			return Address, servers, true
		}
	}
	return Address, nil, false
}

func savedServers(stored record, device string) ([]string, bool) {
	service, found := stored.Devices[device]
	if !found {
		return nil, false
	}
	servers, found := stored.Services[service]
	return slices.Clone(servers), found
}
