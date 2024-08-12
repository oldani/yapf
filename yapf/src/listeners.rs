use anyhow::{Context, Result};
use std::os::unix::net::SocketAddr;
use tokio::net::TcpSocket;

// TODO: configurable backlog
const LISTENER_BACKLOG: u32 = 65535;

fn from_raw_fd(address: &SocketAddr, fd: i32) -> Result<TcpSocket> {
    let std_listener_socket = unsafe { std::net::TcpStream::from_raw_fd(fd) };
    let listener_socket = TcpSocket::from_std_stream(std_listener_socket);
    // Note that we call listen on an already listening socket
    // POSIX undefined but on Linux it will update the backlog size
    Ok(listener_socket.listen(LISTENER_BACKLOG)?)
    // .or_err_with(BindError, || format!("Listen() failed on {address:?}"))?)
}

async fn bind_tcp(addr: &str) -> Result<TcpSocket> {
    let sock_addr = addr
        .to_socket_addrs() // NOTE: this could invoke a blocking network lookup
        .or_err_with(BindError, || format!("Invalid listen address {addr}"))?
        .next() // take the first one for now
        .unwrap(); // assume there is always at least one

    let listener_socket = match sock_addr {
        SocketAddr::V4(_) => TcpSocket::new_v4(),
        SocketAddr::V6(_) => TcpSocket::new_v6(),
    }
    .or_err_with(BindError, || format!("fail to create address {sock_addr}"))?;

    // NOTE: this is to preserve the current TcpListener::bind() behavior.
    // We have a few tests relying on this behavior to allow multiple identical
    // test servers to coexist.
    listener_socket
        .set_reuseaddr(true)
        .or_err(BindError, "fail to set_reuseaddr(true)")?;

    // apply_tcp_socket_options(&listener_socket, opt.as_ref())?;
    // listener_socket.bind(sock_addr).

    match listener_socket.bind(sock_addr) {
        Ok(()) => {
            return Ok(listener_socket
                .listen(LISTENER_BACKLOG)
                .or_err(BindError, "bind() failed")?
                .into())
        }
        Err(e) => {
            if e.kind() != ErrorKind::AddrInUse {
                return Err(e).or_err_with(BindError, || format!("bind() failed on {addr}"));
            }
            try_count += 1;
            if try_count >= TCP_LISTENER_MAX_TRY {
                return Err(e).or_err_with(BindError, || {
                    format!("bind() failed, after retries, {addr} still in use")
                });
            }
        }
    }
}
