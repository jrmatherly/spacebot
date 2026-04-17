"""Stub MCP server for Spacebot client wiring tests.

Exposes two tools via streamable HTTP transport on port 3001:
  - ping: returns "pong"
  - echo: returns the input string verbatim

This is a test fixture, not a real MCP server. It exists to let Spacebot's
MCP client connect to a known-good endpoint without reaching a real external
service.
"""

from fastmcp import FastMCP

mcp = FastMCP("spacebot-mcp-stub")


@mcp.tool()
def ping() -> str:
    """Return "pong". Used to verify basic MCP client wiring."""
    return "pong"


@mcp.tool()
def echo(message: str) -> str:
    """Return the input message verbatim. Used to verify arg passing."""
    return message


if __name__ == "__main__":
    # Streamable HTTP transport on :3001, accessible as http://mcp-stub:3001
    # inside the mcp-net Docker network.
    mcp.run(transport="streamable-http", host="0.0.0.0", port=3001)
