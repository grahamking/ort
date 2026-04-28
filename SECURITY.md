tl;dr `ort` uses rustls and the platform certificate store for TLS. Your prompts and responses are still sent to remote model providers, so do not use hosted models for data that must remain private from the provider.

# Transport Security

`ort` uses rustls for TLS and validates server certificates using the native certificate store through `rustls-native-certs`.

If you set `settings.dns` in the config file, `ort` connects to that IP address directly but still uses the service hostname for TLS SNI and certificate verification. This keeps certificate validation intact while allowing a DNS lookup shortcut or override.

# API Keys

Your OpenRouter or NVIDIA API key is sent to the selected provider over TLS. You should still limit the damage from accidental key exposure:

- Set a small daily credit limit on API keys where the provider supports it.
- Avoid storing keys in shell snippets, shared dotfiles, logs, or examples.
- Rotate a key if you suspect it was exposed.

# Prompt Privacy

If you use a hosted model, the provider can process and potentially retain your prompts, files, and responses. That is true whether you use `ort`, a browser, or another client.

For high-risk private work, the reliable option is to run a model locally or in infrastructure you control.

# Dependencies

The current TLS and certificate verification path depends on maintained third-party crates, primarily `rustls`, `rustls-native-certs`, and their transitive crypto/platform dependencies.
