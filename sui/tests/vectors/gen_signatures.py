"""Regenerate the signature vectors in tests/omni_bridge_tests.move.

Pure-python secp256k1 + keccak256 (needs pycryptodome). The Borsh encoders
below must stay byte-identical to sources/bridge_types.move; every signature
is self-checked by recovering it.
"""
import hmac, hashlib
from Crypto.Hash import keccak

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8


def inv(a, m=P):
    return pow(a, -1, m)


def add(p, q):
    if p is None:
        return q
    if q is None:
        return p
    if p[0] == q[0] and (p[1] + q[1]) % P == 0:
        return None
    if p == q:
        lam = 3 * p[0] * p[0] % P * inv(2 * p[1] % P) % P
    else:
        lam = (q[1] - p[1]) % P * inv((q[0] - p[0]) % P) % P
    x = (lam * lam - p[0] - q[0]) % P
    return (x, (lam * (p[0] - x) - p[1]) % P)


def mul(k, p=(GX, GY)):
    r = None
    while k:
        if k & 1:
            r = add(r, p)
        p = add(p, p)
        k >>= 1
    return r


def kec(b):
    h = keccak.new(digest_bits=256)
    h.update(b)
    return h.digest()


def addr_of(priv):
    x, y = mul(priv)
    return kec(x.to_bytes(32, "big") + y.to_bytes(32, "big"))[12:]


def rfc6979_k(priv, h):
    v = b"\x01" * 32
    k = b"\x00" * 32
    pb = priv.to_bytes(32, "big")
    k = hmac.new(k, v + b"\x00" + pb + h, hashlib.sha256).digest()
    v = hmac.new(k, v, hashlib.sha256).digest()
    k = hmac.new(k, v + b"\x01" + pb + h, hashlib.sha256).digest()
    v = hmac.new(k, v, hashlib.sha256).digest()
    while True:
        v = hmac.new(k, v, hashlib.sha256).digest()
        cand = int.from_bytes(v, "big")
        if 1 <= cand < N:
            return cand
        k = hmac.new(k, v + b"\x00", hashlib.sha256).digest()
        v = hmac.new(k, v, hashlib.sha256).digest()


def sign(priv, msg):
    """Sign the RAW message (Sui's ecrecover keccaks internally).

    Returns 65 bytes r||s||v, v = recovery_id + 27, as the NEAR MPC emits.
    """
    h = kec(msg)
    z = int.from_bytes(h, "big")
    k = rfc6979_k(priv, h)
    x1, y1 = mul(k)
    r = x1 % N
    s = inv(k, N) * (z + r * priv) % N
    rec = (y1 & 1) | (2 if x1 >= N else 0)
    if s > N // 2:
        s = N - s
        rec ^= 1
    return r.to_bytes(32, "big") + s.to_bytes(32, "big") + bytes([rec + 27])


# ---- borsh encoders mirroring bridge_types.move ----
def bstr(s: bytes) -> bytes:
    return len(s).to_bytes(4, "little") + s


def metadata_payload(token, name, symbol, decimals):
    return bytes([1]) + bstr(token) + bstr(name) + bstr(symbol) + bytes([decimals])


def transfer_payload(dest_nonce, origin_chain, origin_nonce, token_addr32,
                     amount, recipient32, fee_recipient, message, chain_id):
    b = bytes([0])
    b += dest_nonce.to_bytes(8, "little")
    b += bytes([origin_chain])
    b += origin_nonce.to_bytes(8, "little")
    b += bytes([chain_id])
    b += token_addr32
    b += amount.to_bytes(16, "little")
    b += bytes([chain_id])
    b += recipient32
    if fee_recipient is not None:
        b += bytes([1]) + bstr(fee_recipient)
    else:
        b += bytes([0])
    if message:
        b += bstr(message)
    return b


def movevec(sig, indent="        "):
    out, line = [], []
    for i, byte in enumerate(sig):
        line.append(f"0x{byte:02X},")
        if len(line) == 12:
            out.append(indent + " ".join(line))
            line = []
    if line:
        out.append(indent + " ".join(line))
    return "\n".join(out)

def recover(sig, msg):
    """Inverse of `sign`, used to self-check each vector."""
    r = int.from_bytes(sig[0:32], "big")
    s = int.from_bytes(sig[32:64], "big")
    rec = sig[64] - 27 if sig[64] >= 27 else sig[64]
    z = int.from_bytes(kec(msg), "big")
    x = r + N * (rec >> 1)
    y = pow((pow(x, 3, P) + 7) % P, (P + 1) // 4, P)
    if (y & 1) != (rec & 1):
        y = P - y
    rinv = inv(r, N)
    Q = add(mul(rinv * s % N, (x, y)), mul((N - rinv * z % N) % N))
    return kec(Q[0].to_bytes(32, "big") + Q[1].to_bytes(32, "big"))[12:]


# Well-known go-ethereum test key; its address is `derived_address()`.
PRIV = 0x4C0883A69102937D6231471B5DBB6204FE5129617082792AE468D01A3F362318

# Any key that is NOT the bridge signer, for the wrong-signer negative test.
WRONG_PRIV = 0x1111111111111111111111111111111111111111111111111111111111111111

# `@omni_bridge` is 0x0 under `sui move test`.
ZERO_PKG = "0" * 64
RECIPIENT = (0xB0B).to_bytes(32, "big")  # USER in the tests
CHAIN_ID = 14


def token_addr(module, name):
    return kec(f"{ZERO_PKG}::{module}::{name}".encode())


def main():
    addr = addr_of(PRIV)
    print(f"// derived_address() = 0x{addr.hex()}\n")
    tc = token_addr("test_coin", "TEST_COIN")   # native / locked coin
    tk = token_addr("token", "TOKEN")           # bridged coin

    cases = [
        ("deploy_signature", metadata_payload(b"wrap.testnet", b"Wrapped NEAR", b"wNEAR", 24)),
        ("usdt_sig", metadata_payload(b"usdt.testnet", b"Wrapped NEAR", b"wNEAR", 24)),
        ("deploy_signature_6dec", metadata_payload(b"six.testnet", b"Six Dec", b"SIX", 6)),
        ("fin_signature", transfer_payload(5, 1, 99, tc, 250, RECIPIENT, b"relayer.near", b"", CHAIN_ID)),
        ("overflow_sig", transfer_payload(6, 1, 100, tc, 1 << 64, RECIPIENT, None, b"", CHAIN_ID)),
        ("fin_signature_token", transfer_payload(5, 1, 99, tk, 250, RECIPIENT, b"relayer.near", b"", CHAIN_ID)),
    ]
    for label, payload in cases:
        sig = sign(PRIV, payload)
        assert recover(sig, payload) == addr, f"{label} does not recover"
        print(f"// {label}\n{movevec(sig)}\n")

    # Valid signature, wrong key: exercises the signer check rather than the
    # native decode path.
    wrong_payload = transfer_payload(
        5, 1, 99, tc, 250, RECIPIENT, b"relayer.near", b"", CHAIN_ID
    )
    wrong_sig = sign(WRONG_PRIV, wrong_payload)
    assert recover(wrong_sig, wrong_payload) == addr_of(WRONG_PRIV) != addr
    print(f"// fin_signature_wrong_signer (recovers to 0x{addr_of(WRONG_PRIV).hex()})")
    print(movevec(wrong_sig))


if __name__ == "__main__":
    main()
