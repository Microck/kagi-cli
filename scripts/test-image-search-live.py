#!/usr/bin/env python3
"""Opt-in subscriber smoke test; makes two image searches, never paid API calls.

Requires KAGI_SESSION_TOKEN in the environment. Credentials are never printed,
written to disk, or passed as command-line arguments. Uses a temporary home/cache.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/debug/kagi")
    parser.add_argument("--query", default="sea otter")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    secret = os.environ.get("KAGI_SESSION_TOKEN")
    if not secret:
        parser.error("KAGI_SESSION_TOKEN must be set privately in the environment")
    with tempfile.TemporaryDirectory(prefix="kagi-images-e2e-") as home:
        env = {k: v for k, v in os.environ.items() if not k.startswith("KAGI_")}
        env.update(KAGI_SESSION_TOKEN=secret, HOME=home, USERPROFILE=home,
                   XDG_CONFIG_HOME=home + "/config", XDG_CACHE_HOME=home + "/cache",
                   KAGI_CACHE_DIR=home + "/cache", RUST_LOG="off")

        def run(argv, stdin=None):
            result = subprocess.run([binary, *argv], input=stdin, env=env,
                                    capture_output=True, text=True, timeout=60)
            if secret in result.stdout or secret in result.stderr:
                raise RuntimeError("Credential appeared in output; output withheld")
            if result.returncode:
                raise RuntimeError(result.stderr[:1000])
            return result.stdout

        first = None
        for fmt in ["json", "compact", "pretty", "csv", "toon", "markdown"]:
            text = run(["search", args.query, "--images", "--limit", "3",
                        "--local-cache", "--format", fmt])
            if fmt == "json":
                first = json.loads(text)
                assert len(first["data"]) == 3, "Expected three results for smoke-test query"
                for item in first["data"]:
                    assert item["image_url"].startswith(("http://", "https://"))
                    assert item["source_url"].startswith(("http://", "https://"))
            elif fmt == "compact":
                assert json.loads(text) == first
            else:
                assert first["data"][0]["source_url"] in text
            print(f"PASS CLI {fmt} (first request live, remaining formats cached)")

        request = {
            "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                "name": "kagi_image_search", "arguments": {"query": args.query, "limit": 2},
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "image-smoke", "version": "1.0.0"},
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        }
        response = json.loads(run(["mcp"], json.dumps(request) + "\n"))
        assert "error" not in response, "MCP protocol error"
        result = response["result"]
        if result.get("isError"):
            raise RuntimeError(result["content"][0]["text"])
        data = json.loads(result["content"][0]["text"])
        assert len(data["data"]) == 2
        assert result["structuredContent"] == data
        print("PASS live MCP image search, limit and structured content")


if __name__ == "__main__":
    main()
