# rill bug report

- rill 0.1.0
- platform: macos aarch64
- settings: color "auto", pager "default", 0 alias(es)

## What happened

(describe what you did, what you expected and what happened)

## rill doctor

```json
{
  "server": {
    "endpoint": "http://127.0.0.1:60938",
    "reachable": true,
    "principal": "dev",
    "latency_ms": 0.43175,
    "error": null
  },
  "daemon": {
    "running": true,
    "pid": 46439,
    "version": "0.1.0",
    "dir": "/var/folders/mq/zg4qxz3j5n5gyqg76sfhn59m0000gn/T/.tmpZFTX5V"
  },
  "cache": {
    "objects": 5,
    "bytes": 32,
    "budget_bytes": 1073741824,
    "free_bytes": 140071948288,
    "evictions": 0
  },
  "tiers": {
    "local": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "regional": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "origin": {
      "reads": 0,
      "p50_ms": 0.0,
      "p99_ms": 0.0
    },
    "regional_fallbacks": 0
  },
  "checkouts": [
    {
      "path": "/private/var/folders/mq/zg4qxz3j5n5gyqg76sfhn59m0000gn/T/.tmpM9j76S/everyday",
      "repo": "everyday",
      "view": "eager",
      "base": "0980478bb9ed4fc59816561e4cf33f4c261e7d7143ff013aa1c79c3e827e1423",
      "pending": 1,
      "last_sync_ms": null,
      "sync_error": null
    }
  ],
  "warnings": []
}
```

## Daemon log (last 0 lines)

```

```
