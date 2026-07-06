GOBIN ?= $(shell go env GOPATH)/bin

.PHONY: build install test vet fmt clean

build:
	go build ./...

install:
	go install ./cmd/...

test:
	go test ./...

vet:
	go vet ./...

fmt:
	gofmt -w .

clean:
	rm -f $(addprefix $(GOBIN)/,clu clu-loop clu-input clu-output \
		clu-provider-anthropic clu-provider-openai \
		clu-tool-read clu-tool-write clu-tool-ls clu-tool-bash)
