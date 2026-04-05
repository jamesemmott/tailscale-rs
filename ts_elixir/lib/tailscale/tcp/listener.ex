defmodule Tailscale.Tcp.Listener do
  @typedoc """
  A TCP listener waiting for incoming connections.
  """
  @opaque t :: Tailscale.Native.tcp_listener()
  
  @spec accept(t()) :: {:ok, Tailscale.Tcp.Stream.t()} | {:error, any()}
  @doc """
  Accept an incoming connection on the socket, yielding a connected `Tailscale.Tcp.Stream`.
    
  Blocks until a connection is ready.
  """
  def accept(res) do
    Tailscale.Native.tcp_accept(res)
  end
end