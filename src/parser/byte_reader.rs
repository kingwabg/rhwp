//! 바이너리 데이터 읽기 유틸리티
//!
//! HWP 레코드 내부의 바이너리 필드를 순차적으로 읽기 위한 커서 기반 리더.
//! HWP는 리틀 엔디안, UTF-16LE 문자열을 사용한다.

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{self, Cursor, Read};

/// 바이트 리더 (커서 기반)
pub struct ByteReader<'a> {
    cursor: Cursor<&'a [u8]>,
    len: usize,
}

impl<'a> ByteReader<'a> {
    /// 새 ByteReader 생성
    pub fn new(data: &'a [u8]) -> Self {
        ByteReader {
            cursor: Cursor::new(data),
            len: data.len(),
        }
    }

    /// 현재 읽기 위치
    pub fn position(&self) -> usize {
        self.cursor.position() as usize
    }

    /// 남은 바이트 수
    pub fn remaining(&self) -> usize {
        self.len.saturating_sub(self.position())
    }

    /// 읽기가 끝났는지 확인
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// u8 읽기
    pub fn read_u8(&mut self) -> io::Result<u8> {
        self.cursor.read_u8()
    }

    /// u16 읽기 (LE)
    pub fn read_u16(&mut self) -> io::Result<u16> {
        self.cursor.read_u16::<LittleEndian>()
    }

    /// u32 읽기 (LE)
    pub fn read_u32(&mut self) -> io::Result<u32> {
        self.cursor.read_u32::<LittleEndian>()
    }

    /// i8 읽기
    pub fn read_i8(&mut self) -> io::Result<i8> {
        self.cursor.read_i8()
    }

    /// i16 읽기 (LE)
    pub fn read_i16(&mut self) -> io::Result<i16> {
        self.cursor.read_i16::<LittleEndian>()
    }

    /// i32 읽기 (LE)
    pub fn read_i32(&mut self) -> io::Result<i32> {
        self.cursor.read_i32::<LittleEndian>()
    }

    /// i64 읽기 (LE)
    pub fn read_i64(&mut self) -> io::Result<i64> {
        self.cursor.read_i64::<LittleEndian>()
    }

    /// 지정 길이의 바이트 읽기
    pub fn read_bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// 읽기 위치를 직접 설정
    pub fn set_position(&mut self, pos: usize) {
        self.cursor.set_position(pos as u64);
    }

    /// N 바이트 건너뛰기
    pub fn skip(&mut self, n: usize) -> io::Result<()> {
        let pos = self.cursor.position() + n as u64;
        if pos > self.len as u64 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "skip 범위 초과",
            ));
        }
        self.cursor.set_position(pos);
        Ok(())
    }

    /// HWP 문자열 읽기 (2바이트 길이 접두사 + UTF-16LE)
    ///
    /// 형식: [u16 글자수] + [UTF-16LE 바이트 * 글자수]
    pub fn read_hwp_string(&mut self) -> io::Result<String> {
        let char_count = self.read_u16()? as usize;
        if char_count == 0 {
            return Ok(String::new());
        }
        self.read_utf16_string(char_count)
    }

    /// UTF-16LE 문자열 읽기 (지정 글자 수)
    ///
    /// [Issue #1932] lone surrogate 가 섞인 실문서(별지 서식 계열 DocInfo)를
    /// 한글은 정상 열람하므로 관용(lossy) 디코딩한다 — 손상 code unit 은
    /// U+FFFD 로 치환되고 문서 로드는 계속된다. 엄격 실패로 DocInfo 전체를
    /// 버리는 종전 동작은 한글 대비 과잉 거부였다.
    pub fn read_utf16_string(&mut self, char_count: usize) -> io::Result<String> {
        let byte_count = char_count * 2;
        let bytes = self.read_bytes(byte_count)?;

        let utf16: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        Ok(String::from_utf16_lossy(&utf16))
    }

    /// ColorRef 읽기 (4바이트, 0x00BBGGRR 형식)
    pub fn read_color_ref(&mut self) -> io::Result<u32> {
        self.read_u32()
    }

    /// 나머지 바이트 전부 읽기
    pub fn read_remaining(&mut self) -> io::Result<Vec<u8>> {
        let remaining = self.remaining();
        self.read_bytes(remaining)
    }
}
