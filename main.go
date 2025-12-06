package main

import (
	"upkg/core"
	"upkg/ui"
)

func main() {
	date := core.GetCoreDate()
	ui.DisplayCoreData(date)
}
