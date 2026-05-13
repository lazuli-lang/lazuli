package queues

import "time"

func durationSeconds(value time.Duration) int {
	return int(value / time.Second)
}
