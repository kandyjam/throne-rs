package sysdns

import (
	"encoding/json"
	"os"

	"golang.org/x/sys/unix"
)

// Address is the beta.4/beta.5 lo0 listener. Beta.6 publishes Tun DNS through a
// temporary network service instead, and no longer writes this alias.
// SystemDNSOverride still reports it so the sing-box patch can drop a leftover
// networksetup pointer instead of using it as an upstream resolver.
const Address = legacyAddress

// stateFile is the ownership record beta.4/beta.5 wrote. Beta.6 Apply does not
// write it. RestoreLegacy deletes it after putting those services back.
const stateFile = legacyStateFile

// record is the beta.4/beta.5 ownership file. owned stays empty because this
// core does not take networksetup DNS.
type record struct {
	PID      int                 `json:"pid"`
	Alias    bool                `json:"alias,omitempty"`
	Services map[string][]string `json:"services,omitempty"`
	Devices  map[string]string   `json:"devices,omitempty"`
}

var owned record

func loadRecord() (record, error) {
	var stored record
	data, err := os.ReadFile(stateFile)
	if err != nil {
		return stored, err
	}
	return stored, json.Unmarshal(data, &stored)
}

// alive also matches the command name: the record survives reboots, where its PID often belongs to someone else.
func alive(pid int) bool {
	if pid <= 0 {
		return false
	}
	owner, err := unix.SysctlKinfoProc("kern.proc.pid", pid)
	if err != nil || int(owner.Proc.P_pid) != pid {
		return false
	}
	self, err := unix.SysctlKinfoProc("kern.proc.pid", os.Getpid())
	return err == nil && owner.Proc.P_comm == self.Proc.P_comm
}
