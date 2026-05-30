package args

import (
	"strings"
)

type Args struct {
	Endpoint string
	ApiKey   string
	Model    string
	Prompt   string
}

func (a *Args) Parse(args []string) error {
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if strings.HasPrefix(arg, "--") {
			opt := arg[2:]
			switch opt {
			case "endpoint":
				i++
				a.Endpoint = args[i]
			case "api-key":
				i++
				a.ApiKey = args[i]
			case "model":
				i++
				a.Model = args[i]
			}
		} else {
			if a.Prompt != "" {
				a.Prompt += " "
			}
			a.Prompt += arg
		}
	}
	return nil
}
