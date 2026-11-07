use sealcore::{open, point_le_to_be, seal, Point};
use substrate_bn::Fr;

fn dec<const N: usize>(s: &str, name: &str) -> Result<[u8; N], String> {
    let v = hex::decode(s.trim_start_matches("0x")).map_err(|e| format!("{name}: bad hex: {e}"))?;
    v.try_into().map_err(|v: Vec<u8>| format!("{name}: need {N} bytes, got {}", v.len()))
}

fn encode_order(side: u8, price_bp: u16, amount: u128, filler: &[u8; 12]) -> Result<[u8; 32], String> {
    if side > 1 {
        return Err("side must be 0 or 1".into());
    }
    if price_bp == 0 || price_bp >= 10_000 {
        return Err("price_bp must be 1..9999".into());
    }
    let mut o = [0u8; 32];
    o[0] = 1;
    o[1] = side;
    o[2..4].copy_from_slice(&price_bp.to_be_bytes());
    o[4..20].copy_from_slice(&amount.to_be_bytes());
    o[20..].copy_from_slice(filler);
    Ok(o)
}

// sealcore::scalar_from_le goes through substrate-bn Fr::from_slice, which does not read a plain
// number (its round trip with scalar_to_le fails), so build the scalar with Fr::interpret instead.
fn scalar_from_le_canonical(le: &[u8; 32]) -> Result<Fr, String> {
    let mut w = [0u8; 64];
    for i in 0..32 {
        w[63 - i] = le[i];
    }
    let k = Fr::interpret(&w);
    if k == Fr::zero() {
        return Err("key is zero".into());
    }
    Ok(k)
}

fn random_scalar() -> Result<Fr, String> {
    let mut w = [0u8; 64];
    getrandom::getrandom(&mut w).map_err(|e| e.to_string())?;
    let r = Fr::interpret(&w);
    if r == Fr::zero() { return random_scalar(); }
    Ok(r)
}

fn do_seal(lock_le: &str, side: &str, price: &str, amount: &str, r: Fr, filler: [u8; 12]) -> Result<String, String> {
    let lock_le: Point = dec(lock_le, "lock")?;
    let order = encode_order(
        side.parse().map_err(|_| "bad side")?,
        price.parse().map_err(|_| "bad price_bp")?,
        amount.parse().map_err(|_| "bad amount_yocto")?,
        &filler,
    )?;
    let (c1, ct) = seal(&point_le_to_be(&lock_le), &r, &order);
    Ok(format!("{{\"c1\":\"{}\",\"ct\":\"{}\"}}", hex::encode(c1), hex::encode(ct)))
}

fn do_open(key: &str, c1: &str, ct: &str) -> Result<String, String> {
    let key: [u8; 32] = dec(key, "key")?;
    let k = scalar_from_le_canonical(&key)?;
    let c1: Point = dec(c1, "c1")?;
    let ct: [u8; 32] = dec(ct, "ct")?;
    let o = open(&k, &c1, &ct).ok_or("c1 is not a valid curve point")?;
    if o[0] != 1 || o[1] > 1 {
        return Err("decoded bytes are not a valid order (wrong key?)".into());
    }
    let price = u16::from_be_bytes([o[2], o[3]]);
    let amount = u128::from_be_bytes(o[4..20].try_into().unwrap());
    Ok(format!("{{\"side\":{},\"price_bp\":{},\"amount_yocto\":\"{}\"}}", o[1], price, amount))
}

fn run(a: &[String]) -> Result<String, String> {
    match a.first().map(|s| s.as_str()) {
        Some("seal") if a.len() == 5 => {
            let mut f = [0u8; 12];
            getrandom::getrandom(&mut f).map_err(|e| e.to_string())?;
            do_seal(&a[1], &a[2], &a[3], &a[4], random_scalar()?, f)
        }
        Some("open") if a.len() == 4 => do_open(&a[1], &a[2], &a[3]),
        _ => Err("usage:\n  sealcli seal <lock_hex_le128> <side 0|1> <price_bp> <amount_yocto>\n  sealcli open <key_hex_le64> <c1_hex> <ct_hex>".into()),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match run(&a) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sealcore::lock_le;
    #[test]
    fn round_trip() {
        let mut key_le = [0u8; 32];
        key_le[0] = 0x39; key_le[1] = 0x05; key_le[20] = 0x77;
        let k = scalar_from_le_canonical(&key_le).unwrap();
        let lock = hex::encode(lock_le(&k));
        let r = Fr::interpret(&[3u8; 64]);
        let out = do_seal(&lock, "1", "6500", "3000000000000000000000000", r, [7; 12]).unwrap();
        let v: serde_like::V = serde_like::parse(&out);
        let key = hex::encode(key_le);
        println!("lock={lock}
out={out}
key={key}"); let got = do_open(&key, &v.c1, &v.ct).unwrap();
        assert_eq!(got, "{\"side\":1,\"price_bp\":6500,\"amount_yocto\":\"3000000000000000000000000\"}");
        assert!(do_seal(&lock, "2", "6500", "1", Fr::interpret(&[4u8; 64]), [0; 12]).is_err());
    }
    mod serde_like {
        pub struct V { pub c1: String, pub ct: String }
        pub fn parse(s: &str) -> V {
            let g = |k: &str| { let p = s.find(&format!("\"{k}\":\"")).unwrap() + k.len() + 4; s[p..p + s[p..].find('"').unwrap()].to_string() };
            V { c1: g("c1"), ct: g("ct") }
        }
    }
}
