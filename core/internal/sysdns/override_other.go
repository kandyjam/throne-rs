//go:build !darwin

package sysdns

type DNSOverride struct{}

func (DNSOverride) SystemDNSOverride(string) (string, []string, bool) { return "", nil, false }
