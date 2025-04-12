#[inline]
pub fn read_le_u8(buf: &mut &[u8]) -> Result<u8, usize> {
    let Some((&[b], rest)) = buf.split_at_checked(1) else {
        return Err(1);
    };
    *buf = rest;
    Ok(b)
}

#[inline]
pub fn read_le_u16(buf: &mut &[u8]) -> Result<u16, usize> {
    let Some((&[a, b], rest)) = buf.split_at_checked(2) else {
        return Err(2 - buf.len());
    };
    *buf = rest;
    Ok(u16::from_le_bytes([a, b]))
}

#[inline]
pub fn read_le_u32(buf: &mut &[u8]) -> Result<u32, usize> {
    let Some((&[a, b, c, d], rest)) = buf.split_at_checked(4) else {
        return Err(4 - buf.len());
    };
    *buf = rest;
    Ok(u32::from_le_bytes([a, b, c, d]))
}

#[inline]
pub fn read_le_u64(buf: &mut &[u8]) -> Result<u64, usize> {
    let Some((&[a, b, c, d, e, f, g, h], rest)) = buf.split_at_checked(8) else {
        return Err(8 - buf.len());
    };
    *buf = rest;
    Ok(u64::from_le_bytes([a, b, c, d, e, f, g, h]))
}
