"""Bearer tokens from `token_provider=`: a Python callable, a source made in Rust,
and the client crate's default source."""

import asyncio
import time

from demo._core import DemoClient, DemoError

# The server's URL, which `run_python` in src/tests.rs sets.
BASE: str = globals()["BASE"]

calls = []


def provider():
    calls.append(1)
    return "provided"


# A bare token is asked for at every request.
client = DemoClient(BASE, token_provider=provider)
assert client.widget("token").get().name == "Bearer provided"
client.widget("token").get()
assert len(calls) == 2, calls

# A token with its expiry is kept until a minute before it.
calls.clear()


def pair():
    calls.append(1)
    return ("kept", time.time() + 3600)


client = DemoClient(BASE, token_provider=pair)
assert client.widget("token").get().name == "Bearer kept"
client.widget("token").get()
assert len(calls) == 1, calls

calls.clear()


def expiring():
    calls.append(1)
    return ("old", time.time() + 30)


client = DemoClient(BASE, token_provider=expiring)
client.widget("token").get()
client.widget("token").get()
assert len(calls) == 2, calls


# The async methods ask the provider too.
async def main():
    return await DemoClient(BASE, token_provider=provider).widget("token").get_async()


assert asyncio.run(main()).name == "Bearer provided"


# A provider that fails, or returns something else, fails the call.
def boom():
    raise RuntimeError("no token today")


try:
    DemoClient(BASE, token_provider=boom).widget("token").get()
    raise AssertionError("a failing provider passed")
except DemoError as e:
    assert "no token today" in str(e), str(e)
try:
    DemoClient(BASE, token_provider=lambda: 7).widget("token").get()
    raise AssertionError("a bad provider passed")
except DemoError as e:
    assert "token_provider" in str(e), str(e)

# Not both.
try:
    DemoClient(BASE, token="t", token_provider=provider)
    raise AssertionError("both passed")
except ValueError:
    pass

# A source made in Rust, from a hand-written submodule.
from demo._core import TokenSource
from demo._core.extra import counting_source

source = counting_source()
assert isinstance(source, TokenSource)
client = DemoClient(BASE, token_provider=source)
assert client.widget("token").get().name == "Bearer t1"
assert client.widget("token").get().name == "Bearer t2"

# Without either, the default source of the client crate: by the host here.
assert DemoClient(BASE).widget("token").get().name == "none"
assert (
    DemoClient(BASE.replace("127.0.0.1", "localhost")).widget("token").get().name
    == "Bearer defaulted"
)
try:
    DemoClient("http://fail.invalid:1/")
    raise AssertionError("a failing default source passed")
except DemoError as e:
    assert "no default token" in str(e), str(e)
