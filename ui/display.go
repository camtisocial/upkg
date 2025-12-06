package ui

import (
	"fmt"
	"upkg/core"
)

func DisplayCoreData(data core.CoreData) {
	fmt.Println("───────── System Update Status ─────────")
	fmt.Printf("Days Since Last Update: %d\n", data.DaysSinceUpdate)
	fmt.Printf("Total Packages Installed: %d\n", data.TotalPackagesInstalled)
	fmt.Printf("Pending Updates: %d\n", data.PendingUpdates)

	barWidth := 20
	filled := data.PendingUpdates
	if filled > barWidth {
		filled = barWidth
	}
	empty := barWidth - filled

	bar := fmt.Sprintf("[%s%s]", repeat("█", filled), repeat("░", empty))
	fmt.Printf("Updates available      : %s\n", bar)
	fmt.Println("───────────────────────────────────────")

}


func repeat(s string, n int) string {
	result := ""
	for i := 0; i < n; i++ {
		result += s
	}
	return result
}	
