defmodule Tailscale.Tcp.Stream do
  @typedoc """
  A handle to a TCP stream (connected socket).
  """
  @opaque t :: Tailscale.Native.tcp_stream()
  
  @spec send(t(), binary()) :: {:ok, integer()} | {:error, any()}
  @doc """
  Send data over the TCP socket, blocking until at least one byte can be sent.
  
  Returns the number of bytes actually sent.
  """
  def send(res, msg) do
    Tailscale.Native.tcp_send(res, msg)
  end
  
  @spec send_all(t(), binary()) :: :ok | {:error, any()}
  @doc """
  Send the payload over the TCP socket, blocking until it is fully sent.
  """
  def send_all(res, msg) do
    len = byte_size(msg)
    
    case Tailscale.Tcp.Stream.send(res, msg) do
      {:ok, ^len} -> :ok
      {:ok, n} -> Tailscale.Tcp.Stream.send_all(res, binary_slice(msg, n..len))
      err -> err
    end
  end
  
  @spec recv(t()) :: {:ok, binary()} | {:error, any()}
  @doc """
  Receive data from the TCP socket, blocking until at least one byte can be received.
  """
  def recv(res) do
    Tailscale.Native.tcp_recv(res)
  end
end