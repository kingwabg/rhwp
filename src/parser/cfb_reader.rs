//! CFB (Compound File Binary) 컨테이너 읽기
//!
//! HWP 파일의 OLE/CFB 컨테이너를 열고 스트림을 추출한다.
//! - FileHeader: 256바이트, 비압축
//! - DocInfo: 레코드 스트림, 압축 가능
//! - BodyText/Section{N}: 레코드 스트림, 압축 가능
//! - ViewText/Section{N}: 배포용 문서 (암호화 + 압축)
//! - BinData/BIN{XXXX}.{ext}: 바이너리 데이터

use std::io::{Cursor, Read};

/// CFB 컨테이너 리더
pub struct CfbReader {
    compound: cfb::CompoundFile<Cursor<Vec<u8>>>,
}

/// CFB 리더 에러
#[derive(Debug)]
pub enum CfbError {
    /// OLE 컨테이너 열기 실패
    OpenError(String),
    /// 스트림 읽기 실패
    StreamError(String),
    /// 스트림을 찾을 수 없음
    StreamNotFound(String),
    /// 압축 해제 실패
    DecompressError(String),
}

impl std::fmt::Display for CfbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CfbError::OpenError(e) => write!(f, "CFB 열기 실패: {}", e),
            CfbError::StreamError(e) => write!(f, "스트림 읽기 실패: {}", e),
            CfbError::StreamNotFound(name) => write!(f, "스트림 없음: {}", name),
            CfbError::DecompressError(e) => write!(f, "압축 해제 실패: {}", e),
        }
    }
}

impl std::error::Error for CfbError {}

impl CfbReader {
    /// 바이트 데이터에서 CFB 컨테이너 열기
    pub fn open(data: &[u8]) -> Result<Self, CfbError> {
        let cursor = Cursor::new(data.to_vec());
        let compound =
            cfb::CompoundFile::open(cursor).map_err(|e| CfbError::OpenError(e.to_string()))?;

        Ok(CfbReader { compound })
    }

    /// 스트림 존재 여부 확인
    pub fn has_stream(&self, path: &str) -> bool {
        self.compound.is_stream(path)
    }

    /// 스트림 원본 데이터 읽기 (압축 해제 없이)
    pub fn read_stream_raw(&mut self, path: &str) -> Result<Vec<u8>, CfbError> {
        if !self.compound.is_stream(path) {
            return Err(CfbError::StreamNotFound(path.to_string()));
        }

        let mut stream = self
            .compound
            .open_stream(path)
            .map_err(|e| CfbError::StreamError(format!("{}: {}", path, e)))?;

        let mut data = Vec::new();
        stream
            .read_to_end(&mut data)
            .map_err(|e| CfbError::StreamError(format!("{}: {}", path, e)))?;

        Ok(data)
    }

    /// FileHeader 스트림 읽기 (256바이트, 항상 비압축)
    pub fn read_file_header(&mut self) -> Result<Vec<u8>, CfbError> {
        self.read_stream_raw("/FileHeader")
    }

    /// DocInfo 스트림 읽기 (압축 가능)
    pub fn read_doc_info(&mut self, compressed: bool) -> Result<Vec<u8>, CfbError> {
        let raw = self.read_stream_raw("/DocInfo")?;
        if compressed {
            decompress_stream(&raw)
        } else {
            Ok(raw)
        }
    }

    /// 본문 섹션 스트림 읽기
    ///
    /// 배포용 문서의 경우 ViewText/Section{N}에서 읽는다.
    /// 일반 문서는 BodyText/Section{N}에서 읽는다.
    /// 반환값은 압축 해제된 레코드 데이터.
    ///
    /// 배포용 문서의 경우, 이 함수는 raw 암호화 데이터를 반환한다.
    /// 호출자가 별도로 복호화를 수행해야 한다.
    pub fn read_body_text_section(
        &mut self,
        index: u32,
        compressed: bool,
        distribution: bool,
    ) -> Result<Vec<u8>, CfbError> {
        if distribution {
            // 배포용 문서: ViewText 스트림 (암호화됨 → 호출자가 복호화)
            let viewtext_path = format!("/ViewText/Section{}", index);
            if self.has_stream(&viewtext_path) {
                return self.read_stream_raw(&viewtext_path);
            }
        }

        // 일반 문서: BodyText 스트림
        let bodytext_path = format!("/BodyText/Section{}", index);
        if self.has_stream(&bodytext_path) {
            let raw = self.read_stream_raw(&bodytext_path)?;
            return if compressed {
                decompress_stream(&raw)
            } else {
                Ok(raw)
            };
        }

        // 루트 레벨 Section (구버전 호환)
        let section_path = format!("/Section{}", index);
        if self.has_stream(&section_path) {
            let raw = self.read_stream_raw(&section_path)?;
            return if compressed {
                decompress_stream(&raw)
            } else {
                Ok(raw)
            };
        }

        Err(CfbError::StreamNotFound(format!("Section{}", index)))
    }

    /// BinData 스트림 읽기 (BinData/BIN{XXXX}.{ext})
    pub fn read_bin_data(&mut self, storage_name: &str) -> Result<Vec<u8>, CfbError> {
        let path = format!("/BinData/{}", storage_name);
        self.read_stream_raw(&path)
    }

    /// 본문 섹션 수 계산
    pub fn section_count(&self) -> u32 {
        let mut count = 0;
        loop {
            let has_body = self
                .compound
                .is_stream(&format!("/BodyText/Section{}", count));
            let has_view = self
                .compound
                .is_stream(&format!("/ViewText/Section{}", count));
            let has_root = self.compound.is_stream(&format!("/Section{}", count));

            if has_body || has_view || has_root {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// BinData 스토리지의 스트림 이름 목록
    pub fn list_bin_data(&self) -> Vec<String> {
        let mut names = Vec::new();
        // cfb 크레이트의 walk API로 BinData 하위 항목 탐색
        for entry in self.compound.walk() {
            let path = entry.path().to_string_lossy().replace('\\', "/");
            if path.starts_with("/BinData/") && entry.is_stream() {
                if let Some(name) = path.strip_prefix("/BinData/") {
                    names.push(name.to_string());
                }
            }
        }
        names
    }

    /// 모든 스트림 경로 목록
    pub fn list_streams(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for entry in self.compound.walk() {
            if entry.is_stream() {
                paths.push(entry.path().to_string_lossy().replace('\\', "/"));
            }
        }
        paths
    }

    /// 모든 엔트리(스트림 + 스토리지) 경로와 크기 목록
    pub fn list_all_entries(&self) -> Vec<(String, u64, bool)> {
        let mut entries = Vec::new();
        for entry in self.compound.walk() {
            let path = entry.path().to_string_lossy().replace('\\', "/");
            let size = entry.len();
            let is_stream = entry.is_stream();
            entries.push((path, size, is_stream));
        }
        entries
    }

    /// 미리보기 이미지 스트림 읽기 (PrvImage)
    ///
    /// BMP 또는 GIF 형식의 썸네일 이미지를 반환한다.
    /// 스트림이 없으면 None을 반환한다.
    pub fn read_preview_image(&mut self) -> Option<Vec<u8>> {
        if self.has_stream("/PrvImage") {
            self.read_stream_raw("/PrvImage").ok()
        } else {
            None
        }
    }

    /// 미리보기 텍스트 스트림 읽기 (PrvText)
    ///
    /// UTF-16LE 인코딩된 미리보기 텍스트를 반환한다.
    /// 스트림이 없으면 None을 반환한다.
    pub fn read_preview_text(&mut self) -> Option<String> {
        if !self.has_stream("/PrvText") {
            return None;
        }

        let data = self.read_stream_raw("/PrvText").ok()?;

        // UTF-16LE 디코딩
        if data.len() < 2 {
            return None;
        }

        let text: String = data
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    let code = u16::from_le_bytes([chunk[0], chunk[1]]);
                    char::from_u32(code as u32)
                } else {
                    None
                }
            })
            .collect();

        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// [Task #1001] HWP3 → HWP5 변환본 식별 (휴리스틱).
    ///
    /// HwpSummaryInformation stream 의 text properties 안에 HWP3 시대 (1990-2003)
    /// 의 년 (예: "1998년") 이 포함되어 있으면 변환본으로 판정. 한컴이 HWP3 →
    /// HWP5 변환 시 원본 작성일 / 메모 등을 HwpSummary 의 string field 에 보존
    /// 하는 패턴을 활용.
    ///
    /// 변환본은 ParaShape spacing/margin 값이 HWP3 원본의 2배로 저장되어 있어
    /// 한컴 viewer 가 표시 시 1/2 보정 (HwpUnitChar 단위) 한다. rhwp 도 동일
    /// 보정을 위해 본 식별 신호 사용.
    ///
    /// Misid risk: HwpSummary 의 다른 field 가 우연히 HWP3 시대 년 포함 시
    /// false positive. 그러나 변환본이 아닌 일반 HWP5 의 ParaShape spacing 은
    /// 대부분 0 이라 1/2 보정 영향이 미미함 (시각 회귀 안전).
    pub fn detect_hwp3_variant(&mut self) -> bool {
        // HwpSummaryInformation stream 시도 (다양한 경로)
        let raw = self
            .read_stream_raw("/\u{0005}HwpSummaryInformation")
            .or_else(|_| self.read_stream_raw("\u{0005}HwpSummaryInformation"))
            .or_else(|_| self.read_stream_raw("HwpSummaryInformation"))
            .unwrap_or_default();
        if raw.len() < 16 {
            return false;
        }
        // UTF-16LE 디코딩
        let utf16: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&utf16);
        // HWP3 시대 (1990-2003) 의 년 검출 — HWP5 도입 (2007년) 이전
        (1990..=2003).any(|y| s.contains(&format!("{}년", y)))
    }
}

/// Lenient CFB 리더 (FAT 검증 무시)
///
/// HWP 프로그램이 생성하는 일부 CFB 파일은 표준 cfb 크레이트의
/// FAT 검증("sector 0 pointed to twice")을 통과하지 못한다.
/// 이 구현체는 FAT 중복을 무시하고 스트림을 추출한다.
pub struct LenientCfbReader {
    data: Vec<u8>,
    sector_size: usize,
    /// Directory entries: (name, start_sector, size, obj_type)
    entries: Vec<(String, u32, u64, u8)>,
    /// FAT table
    fat: Vec<u32>,
    /// Mini-stream data
    mini_stream: Vec<u8>,
    /// Mini-FAT table
    mini_fat: Vec<u32>,
    /// Mini-stream cutoff size
    mini_stream_cutoff: u32,
}

impl LenientCfbReader {
    const END_OF_CHAIN: u32 = 0xFFFFFFFE;
    const FREE_SECT: u32 = 0xFFFFFFFF;

    pub fn open(data: &[u8]) -> Result<Self, CfbError> {
        if data.len() < 512 {
            return Err(CfbError::OpenError("파일이 너무 작음".into()));
        }
        // Magic number 확인
        if &data[0..8] != b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1" {
            return Err(CfbError::OpenError("CFB 매직 넘버 불일치".into()));
        }

        let sector_size_power = u16::from_le_bytes([data[30], data[31]]) as usize;
        let sector_size = 1usize << sector_size_power;
        let mini_sector_size_power = u16::from_le_bytes([data[32], data[33]]) as usize;
        let _mini_sector_size = 1usize << mini_sector_size_power;

        let fat_sectors_count =
            u32::from_le_bytes([data[44], data[45], data[46], data[47]]) as usize;
        let first_dir_sector = u32::from_le_bytes([data[48], data[49], data[50], data[51]]);
        let mini_stream_cutoff = u32::from_le_bytes([data[56], data[57], data[58], data[59]]);
        let first_mini_fat_sector = u32::from_le_bytes([data[60], data[61], data[62], data[63]]);
        let mini_fat_sectors_count =
            u32::from_le_bytes([data[64], data[65], data[66], data[67]]) as usize;
        let first_difat_sector = u32::from_le_bytes([data[68], data[69], data[70], data[71]]);
        let difat_sectors_count =
            u32::from_le_bytes([data[72], data[73], data[74], data[75]]) as usize;

        // DIFAT 읽기: 헤더의 109개 + 추가 DIFAT 섹터
        let mut fat_sector_ids = Vec::new();
        for i in 0..109.min(fat_sectors_count) {
            let off = 76 + i * 4;
            let sid = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
            if sid != Self::FREE_SECT && sid != Self::END_OF_CHAIN {
                fat_sector_ids.push(sid);
            }
        }
        // 추가 DIFAT 섹터 체인
        if difat_sectors_count > 0 && first_difat_sector != Self::END_OF_CHAIN {
            let mut dsid = first_difat_sector;
            for _ in 0..difat_sectors_count {
                let off = 512 + dsid as usize * sector_size;
                if off + sector_size > data.len() {
                    break;
                }
                let entries_per = sector_size / 4 - 1;
                for i in 0..entries_per {
                    let eoff = off + i * 4;
                    let sid = u32::from_le_bytes([
                        data[eoff],
                        data[eoff + 1],
                        data[eoff + 2],
                        data[eoff + 3],
                    ]);
                    if sid != Self::FREE_SECT && sid != Self::END_OF_CHAIN {
                        fat_sector_ids.push(sid);
                    }
                }
                // 다음 DIFAT 섹터
                let next_off = off + entries_per * 4;
                dsid = u32::from_le_bytes([
                    data[next_off],
                    data[next_off + 1],
                    data[next_off + 2],
                    data[next_off + 3],
                ]);
                if dsid == Self::END_OF_CHAIN || dsid == Self::FREE_SECT {
                    break;
                }
            }
        }

        // FAT 빌드
        let mut fat = Vec::new();
        for &fsid in &fat_sector_ids {
            let off = 512 + fsid as usize * sector_size;
            if off + sector_size > data.len() {
                continue;
            }
            let entries = sector_size / 4;
            for i in 0..entries {
                let eoff = off + i * 4;
                fat.push(u32::from_le_bytes([
                    data[eoff],
                    data[eoff + 1],
                    data[eoff + 2],
                    data[eoff + 3],
                ]));
            }
        }

        // Directory entries 읽기
        let dir_data = Self::read_chain_static(data, &fat, first_dir_sector, sector_size);
        let mut entries = Vec::new();
        let entry_size = 128;
        let n_entries = dir_data.len() / entry_size;
        for i in 0..n_entries {
            let eoff = i * entry_size;
            let name_len = u16::from_le_bytes([dir_data[eoff + 64], dir_data[eoff + 65]]) as usize;
            let name = if name_len > 2 {
                let name_bytes = &dir_data[eoff..eoff + name_len - 2]; // UTF-16LE, exclude null
                String::from_utf16_lossy(
                    &name_bytes
                        .chunks(2)
                        .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                        .collect::<Vec<_>>(),
                )
            } else {
                String::new()
            };
            let obj_type = dir_data[eoff + 66];
            let start_sector = u32::from_le_bytes([
                dir_data[eoff + 116],
                dir_data[eoff + 117],
                dir_data[eoff + 118],
                dir_data[eoff + 119],
            ]);
            let size = u64::from_le_bytes([
                dir_data[eoff + 120],
                dir_data[eoff + 121],
                dir_data[eoff + 122],
                dir_data[eoff + 123],
                dir_data[eoff + 124],
                dir_data[eoff + 125],
                dir_data[eoff + 126],
                dir_data[eoff + 127],
            ]);

            if obj_type == 1 || obj_type == 2 || obj_type == 5 {
                entries.push((name, start_sector, size, obj_type));
            }
        }

        // Mini-FAT 빌드
        let mut mini_fat = Vec::new();
        if mini_fat_sectors_count > 0 && first_mini_fat_sector != Self::END_OF_CHAIN {
            let mfat_data = Self::read_chain_static(data, &fat, first_mini_fat_sector, sector_size);
            for chunk in mfat_data.chunks(4) {
                if chunk.len() == 4 {
                    mini_fat.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
        }

        // Mini-stream: Root entry의 스트림 데이터
        let mini_stream = if !entries.is_empty() && entries[0].3 == 5 {
            Self::read_chain_static(data, &fat, entries[0].1, sector_size)
        } else {
            Vec::new()
        };

        Ok(LenientCfbReader {
            data: data.to_vec(),
            sector_size,
            entries,
            fat,
            mini_stream,
            mini_fat,
            mini_stream_cutoff,
        })
    }

    fn read_chain_static(data: &[u8], fat: &[u32], start: u32, sector_size: usize) -> Vec<u8> {
        let mut result = Vec::new();
        let mut sid = start;
        let mut visited = std::collections::HashSet::new();
        while sid != Self::END_OF_CHAIN && sid != Self::FREE_SECT {
            if !visited.insert(sid) {
                break;
            } // 순환 방지
            let off = 512 + sid as usize * sector_size;
            if off + sector_size > data.len() {
                break;
            }
            result.extend_from_slice(&data[off..off + sector_size]);
            if (sid as usize) < fat.len() {
                sid = fat[sid as usize];
            } else {
                break;
            }
        }
        result
    }

    fn read_mini_stream(&self, start: u32, size: u64) -> Vec<u8> {
        let mini_sector_size = 64usize;
        let mut result = Vec::new();
        let mut sid = start;
        let mut visited = std::collections::HashSet::new();
        while sid != Self::END_OF_CHAIN && sid != Self::FREE_SECT {
            if !visited.insert(sid) {
                break;
            }
            let off = sid as usize * mini_sector_size;
            if off + mini_sector_size > self.mini_stream.len() {
                break;
            }
            result.extend_from_slice(&self.mini_stream[off..off + mini_sector_size]);
            if (sid as usize) < self.mini_fat.len() {
                sid = self.mini_fat[sid as usize];
            } else {
                break;
            }
        }
        result.truncate(size as usize);
        result
    }

    /// 디렉토리 경로로 스트림 내용을 가져온다.
    /// 경로 형식: "FileHeader", "DocInfo", "BodyText/Section0" 등
    fn find_entry_idx(&self, path: &str) -> Option<usize> {
        // 경로 "/" 제거 및 트리 탐색 단순화: 이름으로 검색
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        if parts.is_empty() {
            return None;
        }

        // 간단한 DFS - directory entries가 Red-Black 트리이므로
        // child_id/sibling을 써야 하지만, 이름 기반 단순 매칭으로 충분
        // (HWP 파일은 스트림이 많지 않음)
        let target_name = if parts.len() == 1 {
            parts[0].to_string()
        } else {
            // 마지막 세그먼트를 이름으로 사용
            parts.last().unwrap().to_string()
        };

        // 정확한 경로 매칭이 필요하면 트리 탐색해야 하지만,
        // HWP에서는 이름이 유일하므로 단순 매칭
        self.entries
            .iter()
            .position(|(name, _, _, _)| name == &target_name)
    }

    pub fn read_stream(&self, path: &str) -> Result<Vec<u8>, CfbError> {
        let idx = self
            .find_entry_idx(path)
            .ok_or_else(|| CfbError::StreamNotFound(path.to_string()))?;
        let (_, start, size, obj_type) = &self.entries[idx];
        if *obj_type != 2 {
            return Err(CfbError::StreamError(format!(
                "{}: 스트림이 아님 (type={})",
                path, obj_type
            )));
        }

        if *size < self.mini_stream_cutoff as u64 {
            Ok(self.read_mini_stream(*start, *size))
        } else {
            let mut data = Self::read_chain_static(&self.data, &self.fat, *start, self.sector_size);
            data.truncate(*size as usize);
            Ok(data)
        }
    }

    pub fn has_stream(&self, path: &str) -> bool {
        self.find_entry_idx(path).is_some()
    }

    pub fn read_doc_info(&self, compressed: bool) -> Result<Vec<u8>, CfbError> {
        let raw = self.read_stream("DocInfo")?;
        if compressed {
            decompress_stream(&raw)
        } else {
            Ok(raw)
        }
    }

    pub fn read_body_text_section(
        &self,
        index: u32,
        compressed: bool,
    ) -> Result<Vec<u8>, CfbError> {
        let name = format!("Section{}", index);
        let raw = self.read_stream(&name)?;
        if compressed {
            decompress_stream(&raw)
        } else {
            Ok(raw)
        }
    }

    pub fn list_entries(&self) -> &[(String, u32, u64, u8)] {
        &self.entries
    }

    /// FileHeader 스트림 읽기 (256바이트, 항상 비압축)
    pub fn read_file_header(&self) -> Result<Vec<u8>, CfbError> {
        self.read_stream("FileHeader")
    }

    /// 본문 섹션 스트림 읽기 (배포용 ViewText 지원)
    pub fn read_body_text_section_full(
        &self,
        index: u32,
        compressed: bool,
        distribution: bool,
    ) -> Result<Vec<u8>, CfbError> {
        if distribution {
            let viewtext_name = format!("Section{}", index);
            // ViewText 하위 스트림 탐색
            if self.has_stream(&viewtext_name) {
                return self.read_stream(&viewtext_name);
            }
        }

        let name = format!("Section{}", index);
        let raw = self.read_stream(&name)?;
        if compressed {
            decompress_stream(&raw)
        } else {
            Ok(raw)
        }
    }

    /// 본문 섹션 수 계산
    pub fn section_count(&self) -> u32 {
        let mut count = 0u32;
        loop {
            let name = format!("Section{}", count);
            if self.entries.iter().any(|(n, _, _, _)| n == &name) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

/// zlib/deflate 압축 해제
///
/// HWP는 raw deflate (wbits=-15) 사용. 실패 시 표준 zlib도 시도.
pub fn decompress_stream(data: &[u8]) -> Result<Vec<u8>, CfbError> {
    // raw deflate (wbits=-15) 시도
    use flate2::read::DeflateDecoder;
    let mut decoder = DeflateDecoder::new(data);
    let mut decompressed = Vec::new();
    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => return Ok(decompressed),
        Err(_) => {}
    }

    // 표준 zlib 시도
    use flate2::read::ZlibDecoder;
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => Ok(decompressed),
        Err(e) => Err(CfbError::DecompressError(e.to_string())),
    }
}
