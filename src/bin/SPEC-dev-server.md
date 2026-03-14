# Spec: Mock / test server

## Overview

A server written in Rust that mimics part of `openrouter.ai`, returning hard coded responses. For automated testing, and manual testing when there is no Internet. The client is `ort`, the project in this repository.

## Goals

- Contained in a single file `src/bin/dev_server.rs`.
- TLS 1.3+ HTTP server.
- Supports listing models: `v1/models`.
- Support running a prompt: `v1/chat/completions`.
- Input is validated.
- All responses are hard coded inside `src/bin/dev_server.rs`.

## Non-goals

- Does not run any AI inference. All responses are hard coded.
- Does not make any non-localhost network connections.
- Do not write any tests. The file itself is part of our integration testing plan.
- Performance does not matter.

## Activation

`ort` uses a `Site` (`common/site.rs`) to define the endpoints it connects to. It selects the site based on the name of the binary:
- `ort` -> `OPENROUTER` site
- `nrt` -> `NVIDIA` site

The mapping from binary name to Site is in `src/input/cli.rs`.

Add a new mapping. If the binary is called `drt` it will use a new `Site` called `DEV`. Fields:
- config file: `dev.json`.
- API key env var is `ORT_DEV_API_KEY`.
- host: `localhost`.
- port: 8000
- chat_completions_url: "/v1/chat/completions".
- list_url: "/v1/models".

The only changes you will need to make to the client, to support the server you are building, are:
- New `Site` called `DEV` in `common/site.rs`.
- New mapping from binary name `drt` to site `DEV` in `src/input/cli.rs`.

## TLS server

- Listens on localhost:8000
- Has a self-signed TLS certificate for `localhost`. Generate this and store inline in the file as a string or byte array. The client does not validate the server certificate - that will not change.
- Check that an `Authorization: Bearer XYZ` HTTP header is sent, but does not otherwise validate the API key.
- Support TLS 1.3 or above.

## API endpoints

### GET /v1/models

Accept any valid HTTP request. The `Authorization` header is not required, this endpoint is free and public in the openrouter.ai service we are mimicking.

Response data is returned as chunked transfer encoding, with header: `Transfer-Encoding: chunked.`.

Return the following hard-coded JSON, chunked, in an HTTP 200 response with header `Content-Type: application/json`.
```
{
  "data": [
    {
      "id": "openai/gpt-oss-20b",
      "canonical_slug": "openai/gpt-oss-20b",
      "hugging_face_id": "openai/gpt-oss-20b",
      "name": "OpenAI: gpt-oss-20b",
      "created": 1754414229,
      "description": "gpt-oss-20b is an open-weight 21B parameter model released by OpenAI under the Apache 2.0 license. It uses a Mixture-of-Experts (MoE) architecture with 3.6B active parameters per forward pass, optimized for lower-latency inference and deployability on consumer or single-GPU hardware. The model is trained in OpenAI’s Harmony response format and supports reasoning level configuration, fine-tuning, and agentic capabilities including function calling, tool use, and structured outputs.",
      "context_length": 131072,
      "architecture": {
        "modality": "text->text",
        "input_modalities": [
          "text"
        ],
        "output_modalities": [
          "text"
        ],
        "tokenizer": "GPT",
        "instruct_type": null
      },
      "pricing": {
        "prompt": "0.00000002",
        "completion": "0.0000001",
        "request": "0",
        "image": "0",
        "web_search": "0",
        "internal_reasoning": "0"
      },
      "top_provider": {
        "context_length": 131072,
        "max_completion_tokens": 131072,
        "is_moderated": false
      },
      "per_request_limits": null,
      "supported_parameters": [
        "frequency_penalty",
        "include_reasoning",
        "logit_bias",
        "max_tokens",
        "min_p",
        "presence_penalty",
        "reasoning",
        "reasoning_effort",
        "repetition_penalty",
        "response_format",
        "seed",
        "stop",
        "structured_outputs",
        "temperature",
        "tool_choice",
        "tools",
        "top_k",
        "top_p"
      ],
      "default_parameters": {
        "temperature": null,
        "top_p": null,
        "frequency_penalty": null
      },
      "expiration_date": null
    },
    {
      "id": "openai/gpt-oss-120b",
      "canonical_slug": "openai/gpt-oss-120b",
      "hugging_face_id": "openai/gpt-oss-120b",
      "name": "OpenAI: gpt-oss-120b",
      "created": 1754414231,
      "description": "gpt-oss-120b is an open-weight, 117B-parameter Mixture-of-Experts (MoE) language model from OpenAI designed for high-reasoning, agentic, and general-purpose production use cases. It activates 5.1B parameters per forward pass and is optimized to run on a single H100 GPU with native MXFP4 quantization. The model supports configurable reasoning depth, full chain-of-thought access, and native tool use, including function calling, browsing, and structured output generation.",
      "context_length": 131072,
      "architecture": {
        "modality": "text->text",
        "input_modalities": [
          "text"
        ],
        "output_modalities": [
          "text"
        ],
        "tokenizer": "GPT",
        "instruct_type": null
      },
      "pricing": {
        "prompt": "0.000000039",
        "completion": "0.00000019",
        "request": "0",
        "image": "0",
        "web_search": "0",
        "internal_reasoning": "0"
      },
      "top_provider": {
        "context_length": 131072,
        "max_completion_tokens": null,
        "is_moderated": false
      },
      "per_request_limits": null,
      "supported_parameters": [
        "frequency_penalty",
        "include_reasoning",
        "logit_bias",
        "logprobs",
        "max_tokens",
        "min_p",
        "presence_penalty",
        "reasoning",
        "reasoning_effort",
        "repetition_penalty",
        "response_format",
        "seed",
        "stop",
        "structured_outputs",
        "temperature",
        "tool_choice",
        "tools",
        "top_k",
        "top_logprobs",
        "top_p"
      ],
      "default_parameters": {
        "temperature": null,
        "top_p": null,
        "frequency_penalty": null
      },
      "expiration_date": null
    }
  ]
}
```

### POST /v1/chat/completions

### Input JSON

Do not validate the HTTP headers, aside from ensuring it is a well-formed HTTP request.

The body must parse as JSON and contain these fields with these values/shapes; key order and extra fields are ignored unless stated otherwise.

Validate that the HTTP JSON body contains at least these exact fields (allow extra fields). Validate the field contents like this:
- stream: Must exactly match
- usage: Must exactly match the JSON fields and values, order does not matter.
- reasoning: Must exactly match the JSON fields and values, order does not matter.
- model: Must exactly match one of the two models from the above `list` endpoint: `openai/gpt-oss-20b` or `openai/gpt-oss-120b`.
- messages: Must have the same shape. Must contain two messages, one with role "system" and one with role "user". Each message must contain a "content" field. The value of that "content" field can be anything.

On validation failure return HTTP 400 Bad Request with details of the specific problem, except if the model name is invalid. Invalid model name should return 404 Not Found.

```
POST /v1/chat/completions HTTP/1.1
Host: localhost:8000
Content-Length: 331
Content-Type: application/json
Accept: text/event-stream
User-Agent: ort-openrouter-cli/0.4.3
HTTP-Referer: https://github.com/grahamking/ort
X-Title: ort
Authorization: Bearer <accept anything here as long as the header is present>

{"stream": true, "usage": {"include": true}, "model": "openai/gpt-oss-20b", "reasoning": {"exclude": false, "enabled": true, "effort":"low"}, "messages":[{"role":"system","content":"Make your answer concise but complete. No yapping. Direct professional tone. No emoji."},{"role":"user","content":"What is the capital of France?"}]}
```

### Output

Returns an HTTP 200 response with header `Content-Type: text/event-stream`.

The body contains the following as three separate Server-Sent-Events. These are `data:` lines. Send a final `data: [DONE]` afterwards.

The `model` field should match the model field from the request.

```
{"id":"gen-1773502407-NpIIQ87J9N9tdSLJTtt5","object":"chat.completion.chunk","created":1773502407,"model":"openai/gpt-oss-20b","provider":"Amazon Bedrock","choices":[{"index":0,"delta":{"content":"Paris.","role":"assistant"},"finish_reason":null,"native_finish_reason":null}]}
```

```
{"id":"gen-1773502407-NpIIQ87J9N9tdSLJTtt5","object":"chat.completion.chunk","created":1773502407,"model":"openai/gpt-oss-20b","provider":"Amazon Bedrock","choices":[{"index":0,"delta":{"content":"","role":"assistant"},"finish_reason":"stop","native_finish_reason":"stop"}]}
```

```
{"id":"gen-1773502407-NpIIQ87J9N9tdSLJTtt5","object":"chat.completion.chunk","created":1773502407,"model":"openai/gpt-oss-20b","provider":"Amazon Bedrock","choices":[{"index":0,"delta":{"content":"","role":"assistant"},"finish_reason":"stop","native_finish_reason":"stop"}],"usage":{"prompt_tokens":94,"completion_tokens":15,"total_tokens":109,"cost":0.0000087417,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0,"audio_tokens":0,"video_tokens":0},"cost_details":{"upstream_inference_cost":0.00000883,"upstream_inference_prompt_cost":0.00000658,"upstream_inference_completions_cost":0.00000225},"completion_tokens_details":{"reasoning_tokens":4,"image_tokens":0,"audio_tokens":0}}}
```

## Validation

Between each step use `cargo check --bin=dev_server` and `cargo clippy` to check your work and fix any errors or warnings immediately.

Once it is ready for testing, use the main `ort` project as the client. Here's how:

A. Start the server: `cargo run --bin=dev_server`

B. Compile the client: `cargo build`

C. Name the client binary `drt` so that it picks up the new `Site`: `ln -s target/debug/ort target/debug/drt`.

D. Run the client: `./target/debug/drt -r low -m openai/gpt-oss-20b "What is the capital of France?"`

## Constraints

- Contained in a single file `src/bin/dev_server.rs`.
- Can use reasonable external dependencies.
- Keep the code compact and easy to read. This is a testing tool, it must be simple.
- Keep going until the project is complete. Only ask the user in case of safety concerns. When stuck make reasonable safe assumptions and continue.

