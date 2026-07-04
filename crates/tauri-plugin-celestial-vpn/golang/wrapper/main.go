package main

import "C"

import (
	"github.com/metacubex/mihomo/constant"
)

//export MihomoVersion
func MihomoVersion() *C.char {
	return C.CString(constant.Version)
}

func main() {}
