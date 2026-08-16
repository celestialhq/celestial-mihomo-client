// The xray core, embedded the same way mihomo is.
//
// Android has no second process to spawn: the app is one process, and executing a packaged
// binary would mean legacy APK packaging to get a real file on disk past W^X. Linking the
// core in instead sidesteps all of that, and it costs nothing in fidelity — xray reads the
// `xray.json` this app already generates, unchanged from the desktop one.
//
// Kept as its own Go module rather than folded into the mihomo wrapper. Both cores are large
// programs with overlapping dependency trees, and one module means one resolved version of
// every shared dependency: upgrading one core could then only be done by moving the other.
// Two modules, two shared libraries, two independent graphs.
//
// Nothing here needs the patches the mihomo side needs. xray only opens loopback socks
// inbounds and dials out — no TUN, no file descriptor to own, no netlink monitor to skip.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"strings"
	"sync"
	"unsafe"

	"github.com/xtls/xray-core/core"
	"github.com/xtls/xray-core/infra/conf/serial"

	// Registers every protocol and transport. Without it the config loads into an empty
	// registry and every outbound is rejected as an unknown type.
	_ "github.com/xtls/xray-core/main/distro/all"
)

var (
	mu       sync.Mutex
	instance *core.Instance
)

// StartXray parses configJSON — the same document written to `xray.json` on desktop — and
// starts the core in-process.
//
// Replaces a running instance rather than refusing, so the caller's "stop then start" and
// "start again" paths behave identically and neither can leave two cores holding the same
// inbound ports.
//
// Returns nil on success, or a heap C string the caller must release with FreeString.
//
//export StartXray
func StartXray(configJSON *C.char) *C.char {
	config, err := serial.LoadJSONConfig(strings.NewReader(C.GoString(configJSON)))
	if err != nil {
		return C.CString(err.Error())
	}

	server, err := core.New(config)
	if err != nil {
		return C.CString(err.Error())
	}

	mu.Lock()
	defer mu.Unlock()
	if instance != nil {
		_ = instance.Close()
		instance = nil
	}
	if err := server.Start(); err != nil {
		// A partly started instance still owns whatever it managed to bring up. Nothing else
		// holds a reference to it once this returns, so closing it here is the only chance to
		// release the inbound ports the caller is about to be told it does not have.
		_ = server.Close()
		return C.CString(err.Error())
	}
	instance = server
	return nil
}

// StopXray shuts the core down and releases its inbound ports. Safe to call when nothing is
// running, because the stop paths run on shutdown and on failure alike.
//
//export StopXray
func StopXray() {
	mu.Lock()
	defer mu.Unlock()
	if instance != nil {
		_ = instance.Close()
		instance = nil
	}
}

// XrayVersion reports the linked core's version, so a build can be checked against the
// desktop sidecar rather than assumed to match it.
//
//export XrayVersion
func XrayVersion() *C.char {
	return C.CString(core.Version())
}

// FreeString releases a C string previously returned by this library.
//
//export FreeString
func FreeString(s *C.char) {
	C.free(unsafe.Pointer(s))
}

func main() {}
