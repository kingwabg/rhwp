//! 바이너리 데이터 쓰기 유틸리티
//!
//! HWP 레코드 내부의 바이너리 필드를 순차적으로 쓰기 위한 버퍼 기반 라이터.
//! `ByteReader`의 역방향으로, 리틀 엔디안과 UTF-16LE 문자열을 지원한다.

use byteorder::{LittleEndian, WriteBytesExt};
use std::io;

/// 바이트 라이터 (버퍼 기반)
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    /// 새 ByteWriter 생성
    pub fn new() -> Self {
        ByteWriter { buf: Vec::new() }
    }

    /// 현재 쓰기 위치 (바이트 수)
    pub fn position(&self) -> usize {
        self.buf.len()
    }

    /// u8 쓰기
    pub fn write_u8(&mut self, v: u8) -> io::Result<()> {
        self.buf.write_u8(v)
    }

    /// u16 쓰기 (LE)
    pub fn write_u16(&mut self, v: u16) -> io::Result<()> {
        self.buf.write_u16::<LittleEndian>(v)
    }

    /// u32 쓰기 (LE)
    pub fn write_u32(&mut self, v: u32) -> io::Result<()> {
        self.buf.write_u32::<LittleEndian>(v)
    }

    /// i8 쓰기
    pub fn write_i8(&mut self, v: i8) -> io::Result<()> {
        self.buf.write_i8(v)
    }

    /// i16 쓰기 (LE)
    pub fn write_i16(&mut self, v: i16) -> io::Result<()> {
        self.buf.write_i16::<LittleEndian>(v)
    }

    /// i32 쓰기 (LE)
    pub fn write_i32(&mut self, v: i32) -> io::Result<()> {
        self.buf.write_i32::<LittleEndian>(v)
    }

    /// f64 쓰기 (LE)
    pub fn write_f64(&mut self, v: f64) -> io::Result<()> {
        self.buf.write_f64::<LittleEndian>(v)
    }

    /// 바이트 슬라이스 쓰기
    pub fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        self.buf.extend_from_slice(data);
        Ok(())
    }

    /// HWP 문자열 쓰기 (u16 글자수 + UTF-16LE 바이트)
    ///
    /// `ByteReader::read_hwp_string()`의 역방향.
    /// 형식: [u16 글자수] + [UTF-16LE 바이트 * 글자수]
    pub fn write_hwp_string(&mut self, s: &str) -> io::Result<()> {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        self.write_u16(utf16.len() as u16)?;
        for code_unit in &utf16 {
            self.write_u16(*code_unit)?;
        }
        Ok(())
    }

    /// ColorRef 쓰기 (4바이트, 0x00BBGGRR 형식)
    pub fn write_color_ref(&mut self, color: u32) -> io::Result<()> {
        self.write_u32(color)
    }

    /// 0으로 채운 패딩 쓰기
    pub fn write_zeros(&mut self, count: usize) -> io::Result<()> {
        self.buf.extend(std::iter::repeat(0u8).take(count));
        Ok(())
    }

    /// 내부 버퍼를 소유권 이전하여 반환
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// 내부 버퍼 참조 반환
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}
