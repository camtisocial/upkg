package core

import "time"

type CoreData struct {
	DaysSinceUpdate int
	TotalPackagesInstalled int
	PendingUpdates int
}

func DaysSince(t time.Time) int {
	return int(time.Since(t).Hours() / 24)
}

func GetCoreDate() CoreData {
	lastUpdate := time.Date(2025, 12, 1, 0, 0, 0, 0, time.Local)
	return CoreData{
		DaysSinceUpdate: DaysSince(lastUpdate),
		TotalPackagesInstalled: 150,
		PendingUpdates: 5,
	}
}
