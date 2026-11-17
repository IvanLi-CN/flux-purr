#!/usr/bin/env python3
"""Read-only HIL probe for exhaustion of a device's HTTP socket pool."""

import argparse
import concurrent.futures
import json
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--device-id", required=True)
    parser.add_argument("--requests", type=int, default=20)
    parser.add_argument("--concurrency", type=int, default=1)
    args = parser.parse_args()
    url = urllib.parse.urlsplit(args.base_url)
    if (
        url.scheme != "http"
        or not url.hostname
        or url.username is not None
        or url.password is not None
        or url.query
        or url.fragment
        or url.path not in ("", "/")
        or not 1 <= args.requests <= 100
        or not 1 <= args.concurrency <= 3
    ):
        parser.error("provide an HTTP device origin without credentials, 1..100 requests and 1..3 concurrent readers")
    endpoint = args.base_url.rstrip("/") + "/health"
    # Probes never follow a redirect to a different device or send credentials.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None

    def probe(index, barrier):
        # A new connection per probe exercises TCP admission, not keep-alive.
        opener = urllib.request.build_opener(NoRedirect())
        barrier.wait(timeout=5)
        try:
            with opener.open(endpoint, timeout=3) as response:
                health = json.load(response)
                if health.get("deviceId") != args.device_id:
                    return {"request": index + 1, "error": "device_identity_mismatch"}
                if response.status != 200 or health.get("ok") is not True:
                    raise ValueError("health response is not ready")
            return None
        except (OSError, ValueError, urllib.error.URLError) as error:
            return {"request": index + 1, "error": str(error)}

    failures = []
    passed = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        for start in range(0, args.requests, args.concurrency):
            indices = range(start, min(start + args.concurrency, args.requests))
            barrier = threading.Barrier(len(indices))
            results = [pool.submit(probe, index, barrier) for index in indices]
            batch = [result.result() for result in results]
            failures.extend(result for result in batch if result is not None)
            passed += sum(result is None for result in batch)
            if any(result and result["error"] == "device_identity_mismatch" for result in batch):
                break
            time.sleep(0.05)
    print(json.dumps({"passed": passed, "failed": len(failures), "errors": failures}))
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
