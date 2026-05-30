package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"

	"alfosua.com/clu/args"
)

const EventPrefix = "event:"
const DataPrefix = "data:"

func main() {
	fmt.Println("Hello, world!")

	args := &args.Args{}
	if err := args.Parse(os.Args[1:]); err != nil {
		log.Fatalf("Failed to parse arguments: %v", err)
	}

	payload := make(map[string]any)
	payload["input"] = args.Prompt
	payload["model"] = args.Model
	payload["stream"] = true

	payloadJson, err := json.Marshal(payload)
	if err != nil {
		log.Fatalf("Failed to serialize payload for request: %v", err)
	}

	req, err := http.NewRequest("POST", args.Endpoint, bytes.NewBuffer(payloadJson))
	if err != nil {
		log.Fatalf("Failed to create request: %v", err)
	}

	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", args.ApiKey))
	req.Header.Set("Content-Type", "application/json")

	client := &http.Client{Timeout: 0}
	resp, err := client.Do(req)
	if err != nil {
		log.Fatalf("Failed to send request: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		log.Fatalf("HTTP error at request: %s", resp.Status)
	}

	scanner := bufio.NewScanner(resp.Body)
	var currentEvent string
	tailing := false
	for scanner.Scan() {
		line := scanner.Text()

		switch {
		case strings.HasPrefix(line, EventPrefix):
			event := line[len(EventPrefix)+1:]
			currentEvent = event
		case strings.HasPrefix(line, DataPrefix):
			dataJson := line[len(DataPrefix)+1:]
			var data map[string]any
			if err := json.Unmarshal([]byte(dataJson), &data); err != nil {
				log.Fatalf("Failed to deserialize event data: %v", err)
			}

			switch currentEvent {
			case "response.output_item.added":
				if item, ok := data["item"].(map[string]any); ok {
					itemType := item["type"]
					switch itemType {
					case "reasoning":
						fmt.Print("\x1b[38;5;240m") // dark gray
					case "message":
						fmt.Print("\x1b[38;5;255m") // light gray
					}
				}
				if tailing {
					fmt.Print("\n")
				}
			case "response.output_item.done":
				fmt.Print("\033[0m\n")
				tailing = true
			case "response.reasoning_summary_text.delta":
				fmt.Printf("%s", data["delta"])
			case "response.output_text.delta":
				fmt.Printf("%s", data["delta"])
			}
		}
	}

	if err := scanner.Err(); err != nil {
		log.Fatalf("Scanner error: %v", err)
	}
}
