# Troubleshooting

## Upstream stream-init failures (`500 empty_stream`)

**Symptom:** claw-code exits with `500 empty_stream: upstream stream closed before first payload` or similar upstream stream-init error.

**Root cause:** Upstream provider (Anthropic, OpenAI, other) closed the HTTP connection before sending the first response payload. Common causes:
- Transient network issue between claw-code and provider
- Provider overload / temporary service degradation
- Authentication token expired or invalid
- Rate limit exceeded (even if not visible in response headers)

**Mitigation:**
1. **Check credentials:** Verify `claw whoami` shows the expected provider and account. Re-authenticate if expired.
2. **Wait and retry:** Provider transient issues usually resolve within 30-60 seconds. Wait a minute, then retry the same command.
3. **Check provider status:** Visit the provider's status page (e.g., status.anthropic.com, status.openai.com).
4. **Reduce request size:** If the prompt is large, try a smaller request first to isolate stream-init from context-window failures.
5. **Check network:** Ensure your network connection is stable. If behind a proxy, verify proxy allows streaming responses.

**When to escalate:**
- If stream-init failures persist >10 minutes across multiple requests
- If `claw whoami` fails to authenticate
- If no provider status page shows degradation

**Related pinpoint:** #290 (typed stream-init failure envelope — future improvement for better diagnostics)

---

## Other common failures

*[placeholder for future sections: context-window errors, tool-use failures, session corruption, auto-compaction loops]*
