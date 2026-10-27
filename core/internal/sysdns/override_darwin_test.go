package sysdns

import (
	"context"
	"errors"
	"os"
	"reflect"
	"testing"

	"github.com/sagernet/sing/service"
)

func TestSavedDNSPreservesManagedServiceAndSnapshot(t *testing.T) {
	stored := record{Devices: map[string]string{"en0": "Wi-Fi", "en1": "Ethernet"}, Services: map[string][]string{"Wi-Fi": {"192.0.2.53", "2001:db8::53"}, "Ethernet": nil}}
	got, managed := savedServers(stored, "en0")
	if !managed || !reflect.DeepEqual(got, stored.Services["Wi-Fi"]) {
		t.Fatalf("static DNS lost: %v %v", got, managed)
	}
	got[0] = "203.0.113.1"
	if stored.Services["Wi-Fi"][0] != "192.0.2.53" {
		t.Fatal("provider leaked mutable ownership record")
	}
	if servers, managed := savedServers(stored, "en1"); !managed || len(servers) != 0 {
		t.Fatalf("DHCP service confused with unmanaged: %v %v", servers, managed)
	}
	if _, managed := savedServers(stored, "en2"); managed {
		t.Fatal("unmanaged interface acquired DNS override")
	}
	delete(stored.Services, "Wi-Fi")
	if _, managed := savedServers(stored, "en0"); managed {
		t.Fatal("restored service retained DNS override")
	}
}

func TestDNSOverrideStructuralServiceIdentity(t *testing.T) {
	type dnsOverride = interface {
		SystemDNSOverride(string) (string, []string, bool)
	}
	ctx := service.ContextWith[dnsOverride](context.Background(), DNSOverride{})
	if service.FromContext[interface {
		SystemDNSOverride(string) (string, []string, bool)
	}](ctx) == nil {
		t.Fatal("resolver cannot discover structural DNS override")
	}
}

func TestDNSOverrideUnknownOwnershipNeverClaimsDHCP(t *testing.T) {
	saved := record{PID: 42, Devices: map[string]string{"en0": "Wi-Fi"}, Services: map[string][]string{"Wi-Fi": {"192.0.2.53"}}}
	dhcp := record{PID: 42, Devices: map[string]string{"en0": "Wi-Fi"}, Services: map[string][]string{"Wi-Fi": nil}}
	for _, tc := range []struct {
		name    string
		local   record
		stored  record
		err     error
		live    bool
		alias   string
		servers []string
		known   bool
		loads   int
		checks  int
	}{
		{name: "local owner wins without reading inaccessible record", local: saved, err: os.ErrPermission, alias: Address, servers: []string{"192.0.2.53"}, known: true},
		{name: "live owner preserves static servers", stored: saved, live: true, alias: Address, servers: []string{"192.0.2.53"}, known: true, loads: 1, checks: 1},
		{name: "live owner confirms DHCP", stored: dhcp, live: true, alias: Address, known: true, loads: 1, checks: 1},
		{name: "unreadable root record is unknown", err: &os.PathError{Op: "open", Path: stateFile, Err: os.ErrPermission}, alias: Address, loads: 1},
		{name: "corrupt record is unknown", err: errors.New("invalid record"), alias: Address, loads: 1},
		{name: "stale owner cannot supply static DNS", stored: saved, alias: Address, loads: 1, checks: 1},
		{name: "unknown device never implies DHCP", stored: record{PID: 42}, live: true, alias: Address, loads: 1, checks: 1},
		{name: "missing saved service never implies DHCP", stored: record{PID: 42, Devices: map[string]string{"en0": "Wi-Fi"}}, live: true, alias: Address, loads: 1, checks: 1},
		{name: "absent owner record leaves unmanaged DNS alone", err: &os.PathError{Op: "open", Path: stateFile, Err: os.ErrNotExist}, loads: 1},
		{name: "local owner with missing record remains guarded", local: record{PID: 42}, err: os.ErrNotExist, alias: Address, loads: 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			loads, checks := 0, 0
			alias, servers, known := systemDNSOverride("en0", tc.local, func() (record, error) {
				loads++
				return tc.stored, tc.err
			}, func(pid int) bool {
				checks++
				if pid != tc.stored.PID {
					t.Fatalf("checked wrong owner: %d", pid)
				}
				return tc.live
			})
			if alias != tc.alias || !reflect.DeepEqual(servers, tc.servers) || known != tc.known || loads != tc.loads || checks != tc.checks {
				t.Fatalf("alias=%q servers=%v known=%v loads=%d checks=%d; want %q %v %v %d %d", alias, servers, known, loads, checks, tc.alias, tc.servers, tc.known, tc.loads, tc.checks)
			}
		})
	}
}
