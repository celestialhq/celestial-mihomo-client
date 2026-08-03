package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"unsafe"

	mihomoConstant "github.com/metacubex/mihomo/constant"
	"github.com/metacubex/mihomo/hub"
	"github.com/metacubex/mihomo/hub/executor"
	"github.com/metacubex/mihomo/hub/route"
)

func init() {
	// hub/route/patch_android.go (compiled under our android+cmfa build tags)
	// defaults embed mode to true, which disables the REST API's config/rules
	// mutation endpoints (PUT/PATCH /configs, PATCH /rules, etc. — see
	// hub/route/configs.go's `if !embedMode`). That default targets
	// ClashMetaForAndroid's own architecture, where the Kotlin app mutates
	// config by regenerating YAML and reloading rather than through the REST
	// API. We want full REST API parity with desktop (e.g. mode switching via
	// PATCH /configs), so override it back to false.
	route.SetEmbedMode(false)
}

// StartCore parses configYaml and starts the core in-process — proxies, rules,
// DNS, TUN (if the config's tun.file-descriptor is set), and the external-
// controller HTTP API on externalController (e.g. "127.0.0.1:9090"), the same
// address our existing Rust control-plane client (tauri-plugin-mihomo,
// Protocol::Http) already knows how to talk to.
//
// Returns nil on success, or a heap C string on failure that the caller must
// release via FreeString.
//
//export StartCore
func StartCore(configYaml *C.char, homeDir *C.char, externalController *C.char) *C.char {
	if goHomeDir := C.GoString(homeDir); goHomeDir != "" {
		mihomoConstant.SetHomeDir(goHomeDir)
	}

	var opts []hub.Option
	if extCtl := C.GoString(externalController); extCtl != "" {
		opts = append(opts, hub.WithExternalController(extCtl))
	}

	if err := hub.Parse([]byte(C.GoString(configYaml)), opts...); err != nil {
		return C.CString(err.Error())
	}
	return nil
}

// StopCore shuts down the running core (proxies, TUN, listeners) cleanly.
//
//export StopCore
func StopCore() {
	executor.Shutdown()
}

// FreeString releases a C string previously returned by this library.
//
//export FreeString
func FreeString(s *C.char) {
	C.free(unsafe.Pointer(s))
}

// MihomoVersion returns the embedded mihomo core's version string, mostly
// useful for confirming the embedded build matches expectations.
//
//export MihomoVersion
func MihomoVersion() *C.char {
	return C.CString(mihomoConstant.Version)
}

func main() {}
