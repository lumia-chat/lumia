#!/usr/bin/env python3
"""A relay between the phone and an instance, for scripts/smoke-signout.sh:
it passes every connection through to the instance until told to stall,
and then is an instance that accepts the connection and never answers.

    relay.py <listen port> <instance host> <instance port> <work directory>

While <work>/stall exists nothing passes in either direction: what a
connection sends is read and dropped, a new connection is accepted and left
at that, and a connection that was stalled is closed once the file is gone
(what it swallowed is not delivered late). <work>/gateways holds how many
gateway connections (WebSocket upgrades) are open through the relay, which
is how many gateway loops the phone is running: one per open session.
"""
import os
import socket
import sys
import threading

port, host, target, work = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
stall = os.path.join(work, "stall")
count = os.path.join(work, "gateways")
gateways = set()
lock = threading.Lock()


def stalled():
    return os.path.exists(stall)


def note():
    with open(count + ".new", "w") as f:
        f.write("%d\n" % len(gateways))
    os.replace(count + ".new", count)


def gateway(connection, there):
    with lock:
        if there:
            gateways.add(connection)
        else:
            gateways.discard(connection)
        note()


def close(*sockets):
    for s in sockets:
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        s.close()


def pump(source, sink, connection, asking):
    """Passes what `source` sends to `sink`; `asking` is the phone's side, whose first bytes say what the connection is."""
    first, swallowed = True, False
    try:
        while True:
            data = source.recv(65536)
            if not data:
                break
            if stalled():
                swallowed = True
                continue
            if swallowed:
                break
            if asking and first and b"upgrade: websocket" in data.lower():
                gateway(connection, True)
            first = False
            sink.sendall(data)
    except OSError:
        pass
    finally:
        gateway(connection, False)
        close(source, sink)


def serve(phone):
    if stalled():
        # Accepted, and never answered.
        try:
            while phone.recv(65536):
                pass
        except OSError:
            pass
        close(phone)
        return
    try:
        instance = socket.create_connection((host, target))
    except OSError:
        close(phone)
        return
    threading.Thread(target=pump, args=(phone, instance, phone, True), daemon=True).start()
    threading.Thread(target=pump, args=(instance, phone, phone, False), daemon=True).start()


listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", port))
listener.listen(64)
note()
while True:
    phone, _ = listener.accept()
    threading.Thread(target=serve, args=(phone,), daemon=True).start()
