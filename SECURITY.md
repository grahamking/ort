tl;dr `ort` is not traditionally secure because it is vulnerable to [man-in-the-middle](https://en.wikipedia.org/wiki/Man-in-the-middle_attack) attacks and has an un-reviewed TLS stack. If your prompts are very high risk do not use it. Otherwise, read on!

# Man-in-the-middle attack

`ort` does not validate TLS certificates. This makes the binary smaller and the connection start faster. It also makes it vunlerable to man-in-the-middle attacks.

It's OK because you can:

1. prevent it manually
2. secure the at-risk things in other ways
3. and in this specific case checking the certs wasn't helping anyway, just slowing you down.

The two things at risk here are your OpenRouter API key and your prompts / responses.

## Prevent it manually

You can verify easily enough if there is a man-in-the-middle on your connection by browsing to openrouter.ai to see if your browser shows an alert. The browsers all do check certificates. If it doesn't you're likely fine.

It would be _possible_ to identify an `ort` request vs a browser request, by fingerprinting the TLS `ClientHello`, and then only man-in-the-middle ort and not the browser, but it is extremely unlikely at this stage of the projects life.

## Secure your API key

The biggest risk is a man-in-the-middle gaining your OpenRouter API key. I have a $5 daily limit on mine, which I recommend. Go do that now:

- After login browse to [OpenRouter API Keys](https://openrouter.ai/settings/keys)
- Click the three dots on the right of an API key, select Edit
- Put in a credit limit ($3, $5, something small) and reset that limit Daily

Unless you use an agent such as Claude or OpenCode, your AI spend on openrouter will likely be a few cents a day. I have never hit my $5 daily limit.

You are far more likely to leak your open router key in other ways than a sophisticated man-in-the-middle attack: Accidentally sharing your `.bashrc`, leaving it in a `curl` example, and so on. Go limit the damage now.

## You can't secure your prompts

Your prompts are already likely saved by the provider, so hopefully you weren't expecting those to be secret anyway. The provider will use them to train new models (this is good!). You should assume this even if they say they won't; safety first.

The provider will also deliver them to any legal authority that asks nicely.

## Who could attack you

A man-in-the-middle attack could be perpetrated by the owner of the wi-fi router (so use a VPN when not at home!), your ISP, or a major network provider between your ISP and openrouter.

In practice that means don't use other people's wi-fi without a VPN. This has always been the case for any and all online activity.

## It's a choice, but it's a win

To summarize, we gain a smaller binary and faster requests, in exchange for a very small risk of losing up to $5 a day, that you can mitigate simply by browsing to openrouter.ai. Security engineers hate this one weird trick.

# Other risks

`ort` has it's own TLS 1.3 stack, including all crypto operations: AES-128 GCM, HMAC, HKDF, ECDH/X25519, SHA-256. GPT-5 wrote the bulk of these implementations, one at a time. I wrote the tests first. They have not been reviewed by anyone else.

I have validated all the obvious stuff: We generate a random private/public key pair on every run. The Nonce are unique. We check the GCM AEAD tag. I consider this TLS stack safe.

# I am very high risk. What can I do?

If you are using a model hosted by someone else, whether via `ort` or not doesn't matter, they can read both your prompts and the engine's response. The way LLMs work today there is no way around that. That makes your activity visible to, at a minimum, the people who run the model inference stack and the legal authorities in that country.

The only way reliable way around this is to run the model yourself locally.

If you run it _remotely_ yourself, the machine owner can still see your work. There is an experimental tool called [prompt_embeds](https://docs.nvidia.com/nim/large-language-models/latest/prompt-embeds.html) which might help in the future.

To run a model locally means buying a powerful GPU or two (today, about $10,000 to $15,000 for a great setup) getting the model from [huggingface](https://huggingface.co/) and running an engine such as [llama.cpp](https://github.com/ggml-org/llama.cpp). You won't be able to match the performance of the latest models, but you'll get something fast that will impress you. It's simpler to setup than it sounds.

Sub-reddit [LocalLLaMA](https://www.reddit.com/r/LocalLLaMA/) has lots of advice from people doing this.

