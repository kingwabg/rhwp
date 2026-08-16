//! 한컴 수식 스크립트 재귀 하강 파서
//!
//! 토큰 리스트를 AST(EqNode)로 변환한다.

use super::ast::*;
use super::symbols::{
    self, is_big_operator, is_function, is_structure_command, lookup_function, lookup_symbol,
    FontStyleKind, DECORATIONS, FONT_STYLES,
};
use super::tokenizer::{tokenize, Token, TokenType};

/// 수식 파서
pub struct EqParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl EqParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos).filter(|t| t.ty != TokenType::Eof)
    }

    fn current_type(&self) -> TokenType {
        self.tokens
            .get(self.pos)
            .map(|t| t.ty)
            .unwrap_or(TokenType::Eof)
    }

    fn current_value(&self) -> &str {
        self.tokens
            .get(self.pos)
            .map(|t| t.value.as_str())
            .unwrap_or("")
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.current_type() == TokenType::Eof
    }

    fn advance(&mut self) -> Option<&Token> {
        if self.at_end() {
            return None;
        }
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        Some(tok)
    }

    fn expect(&mut self, ty: TokenType) -> bool {
        if self.current_type() == ty {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// 명령어 대소문자 무시 비교
    fn cmd_eq(val: &str, target: &str) -> bool {
        val.eq_ignore_ascii_case(target)
    }

    /// 최상위 레벨에서 OVER가 있는지 확인 (괄호/LEFT-RIGHT 내부 제외)
    fn has_toplevel_over(tokens: &[Token]) -> bool {
        let mut brace_depth = 0i32;
        let mut lr_depth = 0i32;
        for t in tokens {
            match t.ty {
                TokenType::LBrace => brace_depth += 1,
                TokenType::RBrace => brace_depth -= 1,
                TokenType::Command => {
                    if Self::cmd_eq(&t.value, "LEFT") {
                        lr_depth += 1;
                    } else if Self::cmd_eq(&t.value, "RIGHT") {
                        lr_depth -= 1;
                    } else if Self::cmd_eq(&t.value, "OVER") && brace_depth == 0 && lr_depth == 0 {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// 수식 전체 파싱 (엔트리 포인트)
    pub fn parse(&mut self) -> EqNode {
        self.parse_expression()
    }

    /// OVER/ATOP 중위 연산자 처리. 현재 토큰이 OVER/ATOP 이면 children 의 마지막 요소를
    /// pop 하여 분수/atop 으로 결합한다. 처리했으면 true, 아니면 false.
    /// CASES/PILE/EQALIGN 등 row-collecting 파서가 분수를 인식하지 못하는 결함(#505)을
    /// 방지하기 위해 모든 token-collecting 루프에서 호출한다.
    fn try_consume_infix_over_atop(&mut self, children: &mut Vec<EqNode>) -> bool {
        if self.current_type() != TokenType::Command {
            return false;
        }
        let val = self.current_value();
        let is_over = Self::cmd_eq(val, "OVER");
        let is_atop = Self::cmd_eq(val, "ATOP");
        if !is_over && !is_atop {
            return false;
        }
        self.pos += 1;
        let top = children.pop().unwrap_or(EqNode::Empty);
        let bottom = self.parse_element();
        children.push(if is_atop {
            EqNode::Atop {
                top: Box::new(top),
                bottom: Box::new(bottom),
            }
        } else {
            EqNode::Fraction {
                numer: Box::new(top),
                denom: Box::new(bottom),
            }
        });
        true
    }

    /// 표현식 파싱 (중단 조건 없이 끝까지)
    /// OVER/ATOP을 중위 연산자로 처리: 바로 앞/뒤 요소를 위아래로 배치
    fn parse_expression(&mut self) -> EqNode {
        let mut children = Vec::new();
        while !self.at_end() {
            // 그룹 종료 또는 RIGHT 만나면 중단
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Command
                && Self::cmd_eq(self.current_value(), "RIGHT")
            {
                break;
            }
            // OVER/ATOP 중위 연산자: 직전/직후 요소를 위아래로 결합
            if self.try_consume_infix_over_atop(&mut children) {
                continue;
            }
            children.push(self.parse_element());
        }
        EqNode::Row(children).simplify()
    }

    /// 단일 요소 파싱
    fn parse_element(&mut self) -> EqNode {
        if self.at_end() {
            return EqNode::Empty;
        }

        let ty = self.current_type();
        let val = self.current_value().to_string();

        match ty {
            TokenType::Command => {
                self.pos += 1;
                self.parse_command(&val)
            }
            TokenType::Number => {
                self.pos += 1;
                self.try_parse_scripts(EqNode::Number(val))
            }
            TokenType::Symbol => {
                self.pos += 1;
                // -> 는 →로 변환
                if val == "->" {
                    EqNode::MathSymbol("→".to_string())
                } else {
                    EqNode::Symbol(val)
                }
            }
            TokenType::Text => {
                self.pos += 1;
                self.try_parse_scripts(EqNode::Text(val))
            }
            TokenType::Quoted => {
                self.pos += 1;
                self.try_parse_scripts(EqNode::Quoted(val))
            }
            TokenType::Whitespace => {
                self.pos += 1;
                match val.as_str() {
                    "~" => EqNode::Space(SpaceKind::Normal),
                    "`" => EqNode::Space(SpaceKind::Thin),
                    "#" => EqNode::Newline,
                    "&" => EqNode::Space(SpaceKind::Tab),
                    _ => EqNode::Empty,
                }
            }
            TokenType::LBrace => {
                let group = self.parse_group();
                self.try_parse_scripts(group)
            }
            TokenType::LParen => {
                // #1305: `(...)` 뒤에 첨자가 오면 Paren 그룹으로 묶어 첨자를 결합한다
                // (그렇지 않으면 `)` 뒤 `^2` 가 base 없는 orphan Superscript 가 됨).
                // 첨자가 없으면 기존대로 느슨한 Symbol 흐름 유지 → 일반 괄호 렌더 무변경.
                if self.paren_then_script() {
                    let group = self.parse_paren_group();
                    self.try_parse_scripts(group)
                } else {
                    self.pos += 1;
                    EqNode::Symbol(val)
                }
            }
            TokenType::RParen | TokenType::LBracket | TokenType::RBracket => {
                self.pos += 1;
                EqNode::Symbol(val)
            }
            TokenType::Subscript | TokenType::Superscript => {
                // 베이스 없는 첨자 (예: _{y} x)
                self.try_parse_scripts(EqNode::Empty)
            }
            _ => {
                self.pos += 1;
                EqNode::Empty
            }
        }
    }

    /// 명령어 처리
    fn parse_command(&mut self, cmd: &str) -> EqNode {
        let cmd_upper = cmd.to_ascii_uppercase();
        let cu = cmd_upper.as_str();
        // [#1204] hwpeq 명령은 대소문자 무시 — DECORATIONS/FONT_STYLES 는 소문자 키이므로
        // 1차 lookup 실패 시 소문자 fallback (`RM`/`BAR` 등 대문자 변형이 leak 되지 않도록).
        let cmd_lower = cmd.to_ascii_lowercase();

        // OVER/ATOP은 parse_expression에서 처리됨 (단독 발생 시)
        if cu == "OVER" {
            return EqNode::Empty;
        }

        if cu == "ATOP" {
            return EqNode::Empty;
        }

        // LaTeX 분수: \frac{a}{b}, \dfrac{a}{b}, \tfrac{a}{b}
        if matches!(cu, "FRAC" | "DFRAC" | "TFRAC") {
            return self.parse_latex_fraction();
        }

        // LaTeX \text{...} — 로만체 텍스트
        // 제한: 토크나이저가 일반 공백을 건너뛰므로 \text{a b} 내부 공백은 보존되지 않음.
        // 공백이 필요하면 hwpeq 관례대로 ~ 사용 (\text{if~}).
        if cu == "TEXT" {
            let body = self.parse_single_or_group();
            return EqNode::FontStyle {
                style: FontStyleKind::Roman,
                body: Box::new(body),
            };
        }

        // LaTeX \operatorname{...} — 로만체 연산자명
        if cu == "OPERATORNAME" {
            let body = self.parse_single_or_group();
            return EqNode::FontStyle {
                style: FontStyleKind::Roman,
                body: Box::new(body),
            };
        }

        // LaTeX \phantom{...} — 보이지 않는 공간 (레이아웃 정렬용)
        if matches!(cu, "PHANTOM" | "VPHANTOM" | "HPHANTOM") {
            self.parse_single_or_group();
            return EqNode::Text(" ".to_string());
        }

        // LaTeX spacing: \quad, \qquad, \,, \:, \;, \!
        if matches!(
            cu,
            "QUAD" | "QQUAD" | "THINSPACE" | "MEDSPACE" | "THICKSPACE" | "NEGSPACE" | "ENSPACE"
        ) {
            let space = lookup_symbol(cu).unwrap_or(" ");
            return EqNode::Text(space.to_string());
        }

        // LaTeX \overset{over}{base}, \underset{under}{base}, \stackrel{over}{base}
        if matches!(cu, "OVERSET" | "STACKREL") {
            let over = self.parse_single_or_group();
            let base = self.parse_single_or_group();
            return EqNode::Superscript {
                base: Box::new(base),
                sup: Box::new(over),
            };
        }
        if cu == "UNDERSET" {
            let under = self.parse_single_or_group();
            let base = self.parse_single_or_group();
            return EqNode::Subscript {
                base: Box::new(base),
                sub: Box::new(under),
            };
        }

        // LaTeX \begin{env}...\end{env}
        if cu == "BEGIN" {
            return self.parse_latex_environment();
        }
        if cu == "END" {
            self.skip_brace_arg();
            return EqNode::Empty;
        }

        // 제곱근
        if cu == "SQRT" || cu == "ROOT" {
            return self.parse_sqrt();
        }

        // 적분 기호 — nolimits: 큰 기호 + 일반 첨자 (BigOp이 아닌 MathSymbol로 처리)
        if matches!(
            cu,
            "INT"
                | "INTEGRAL"
                | "SMALLINT"
                | "DINT"
                | "TINT"
                | "OINT"
                | "SMALLOINT"
                | "ODINT"
                | "OTINT"
        ) {
            let symbol = lookup_symbol(cu)
                .or_else(|| lookup_symbol(cmd))
                .unwrap_or("∫")
                .to_string();
            let node = EqNode::MathSymbol(symbol);
            return self.try_parse_scripts(node);
        }

        // 큰 연산자 (∑, ∏ 등) — limits: 기호 위/아래 중앙
        if is_big_operator(cu) {
            let symbol = lookup_symbol(cu).unwrap_or("?").to_string();
            return self.parse_big_op(symbol);
        }
        // 원본 대소문자로도 확인 (대소문자 구분 명령어)
        if is_big_operator(cmd) {
            let symbol = lookup_symbol(cmd).unwrap_or("?").to_string();
            return self.parse_big_op(symbol);
        }

        // 극한 (대소문자 구분)
        if cmd == "lim" || cmd == "Lim" {
            return self.parse_limit(cmd == "Lim");
        }

        // 행렬
        if matches!(cu, "MATRIX" | "PMATRIX" | "BMATRIX" | "DMATRIX") {
            let style = match cu {
                "PMATRIX" => MatrixStyle::Paren,
                "BMATRIX" => MatrixStyle::Bracket,
                "DMATRIX" => MatrixStyle::Vert,
                _ => MatrixStyle::Plain,
            };
            return self.parse_matrix(style);
        }

        // 조건식
        if cu == "CASES" {
            return self.parse_cases();
        }

        // 칸 맞춤 정렬
        if cu == "EQALIGN" {
            return self.parse_eqalign();
        }

        // 세로 쌓기
        if matches!(cu, "PILE" | "LPILE" | "RPILE") {
            let align = match cu {
                "LPILE" => PileAlign::Left,
                "RPILE" => PileAlign::Right,
                _ => PileAlign::Center,
            };
            return self.parse_pile(align);
        }

        // LEFT-RIGHT 괄호
        if cu == "LEFT" {
            // ★ KeepGong fix: 구분기호 그룹(left|...right| 등) 뒤 첨자(^/_)를 그룹 전체에 부착.
            //   기존엔 try_parse_scripts 를 안 거쳐 |x|^3 의 ^3 가 base 없는 고아 첨자가 됐다.
            let node = self.parse_left_right();
            return self.try_parse_scripts(node);
        }

        if cu == "RIGHT" {
            return EqNode::Empty;
        }

        // REL / BUILDREL
        if cu == "REL" || cu == "BUILDREL" {
            let is_buildrel = cu == "BUILDREL";
            // 화살표 기호 읽기 (다음 요소를 파싱하여 화살표로 사용)
            let arrow_node = self.parse_element();
            let arrow = match &arrow_node {
                EqNode::MathSymbol(s) => s.clone(),
                EqNode::Symbol(s) => s.clone(),
                EqNode::Text(s) => s.clone(),
                _ => "→".to_string(),
            };
            // {위 내용}
            let over = self.parse_single_or_group();
            // {아래 내용} (REL만)
            let under = if !is_buildrel {
                Some(Box::new(self.parse_single_or_group()))
            } else {
                None
            };
            return EqNode::Rel {
                arrow,
                over: Box::new(over),
                under,
            };
        }

        // LONGDIV: LONGDIV {제수}{몫}{피제수#나머지...}
        if cu == "LONGDIV" {
            let divisor = self.parse_single_or_group();
            let quotient = self.parse_single_or_group();
            let body = self.parse_single_or_group();
            // 간이 표현: 몫 위에 줄, 제수)피제수 형태
            return EqNode::Row(vec![
                quotient,
                EqNode::Symbol("÷".to_string()),
                divisor,
                EqNode::Symbol("=".to_string()),
                body,
            ]);
        }

        // LADDER / SLADDER: 사다리꼴 레이아웃 → Matrix로 fallback
        if cu == "LADDER" || cu == "SLADDER" {
            return self.parse_matrix(MatrixStyle::Plain);
        }

        // BENZENE: 벤젠 분자 구조 → 텍스트 placeholder
        if cu == "BENZENE" {
            return EqNode::MathSymbol("⌬".to_string());
        }

        // BIGG: 크기 확대 (현재 크기 변경 무시, 내부 요소만 반환)
        if cu == "BIGG" {
            let inner = self.parse_element();
            return inner;
        }

        // CHOOSE / BINOM
        if cu == "CHOOSE" {
            // n CHOOSE r → 이전 요소와 다음 요소를 조합으로
            let body = self.parse_single_or_group();
            return EqNode::Paren {
                left: "(".to_string(),
                right: ")".to_string(),
                body: Box::new(EqNode::Atop {
                    top: Box::new(EqNode::Empty), // 이전 요소는 상위에서 처리
                    bottom: Box::new(body),
                }),
            };
        }

        if cu == "BINOM" {
            let top = self.parse_single_or_group();
            let bottom = self.parse_single_or_group();
            return EqNode::Paren {
                left: "(".to_string(),
                right: ")".to_string(),
                body: Box::new(EqNode::Atop {
                    top: Box::new(top),
                    bottom: Box::new(bottom),
                }),
            };
        }

        // 색상
        if cu == "COLOR" {
            return self.parse_color();
        }

        // 왼쪽 첨자
        if cu == "LSUB" || cu == "LSUP" {
            let script = self.parse_single_or_group();
            let body = self.parse_single_or_group();
            if cu == "LSUB" {
                return EqNode::Subscript {
                    base: Box::new(body),
                    sub: Box::new(script),
                };
            } else {
                return EqNode::Superscript {
                    base: Box::new(body),
                    sup: Box::new(script),
                };
            }
        }

        // SUP/SUB 동의어
        if cu == "SUP" {
            let body = self.parse_single_or_group();
            return self.try_parse_scripts(body);
        }
        if cu == "SUB" {
            let body = self.parse_single_or_group();
            return self.try_parse_scripts(body);
        }

        // 글자 장식
        if let Some(&deco) = DECORATIONS
            .get(cmd)
            .or_else(|| DECORATIONS.get(cmd_lower.as_str()))
        {
            let body = self.parse_single_or_group();
            return EqNode::Decoration {
                kind: deco,
                body: Box::new(body),
            };
        }

        // 글꼴 스타일
        if let Some(&style) = FONT_STYLES
            .get(cmd)
            .or_else(|| FONT_STYLES.get(cmd_lower.as_str()))
        {
            // 다음 토큰이 구조 명령어(LEFT, RIGHT 등)이면 body 없이 반환
            // rm P it LEFT(...) 에서 it이 LEFT를 body로 먹지 않도록
            let body = if self.current_type() == TokenType::Command
                && is_structure_command(&self.current_value().to_ascii_uppercase())
            {
                EqNode::Empty
            } else {
                self.parse_single_or_group()
            };
            return EqNode::FontStyle {
                style,
                body: Box::new(body),
            };
        }

        // Unicode 기호 매핑 — 함수보다 우선 (hwpeq inf=∞ vs LaTeX \inf=infimum 충돌 방지)
        if let Some(symbol) = lookup_symbol(cmd) {
            let node = EqNode::MathSymbol(symbol.to_string());
            return self.try_parse_scripts(node);
        }

        // 함수 (sin, cos, log 등)
        if is_function(cmd) {
            let func_name = lookup_function(cmd).unwrap_or(cmd).to_string();
            if self.current_type() == TokenType::Whitespace && self.current_value() == "`" {
                self.pos += 1;
            }
            let node = EqNode::Function(func_name);
            return self.try_parse_scripts(node);
        }

        // 알 수 없는 명령어 → 텍스트로 처리
        let node = EqNode::Text(cmd.to_string());
        self.try_parse_scripts(node)
    }

    /// 중괄호 그룹 파싱: {...}
    /// 그룹 내의 OVER는 parse_expression의 중위 연산자 처리로 자동 처리된다.
    fn parse_group(&mut self) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return self.parse_element();
        }

        let mut children = Vec::new();
        while !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            // OVER/ATOP 중위 연산자: 그룹 내에서도 동일하게 처리
            if self.try_consume_infix_over_atop(&mut children) {
                continue;
            }
            children.push(self.parse_element());
        }

        // 닫는 괄호 건너뛰기
        self.expect(TokenType::RBrace);

        EqNode::Row(children).simplify()
    }

    /// 매칭되는 닫는 괄호 위치 찾기
    fn find_matching_brace(&self, start: usize) -> usize {
        let mut depth = 1i32;
        let mut pos = start;
        while pos < self.tokens.len() {
            match self.tokens[pos].ty {
                TokenType::LBrace => depth += 1,
                TokenType::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return pos;
                    }
                }
                _ => {}
            }
            pos += 1;
        }
        self.tokens.len()
    }

    /// 현재 LParen 의 매칭 RParen 다음 토큰이 첨자(`^`/`_`)인지 (#1305).
    /// 참이면 `(...)` 를 Paren 그룹으로 묶어 첨자를 결합해야 한다.
    /// 현재 토큰이 LParen 이라는 전제.
    fn paren_then_script(&self) -> bool {
        let mut depth = 0i32;
        let mut p = self.pos;
        while p < self.tokens.len() {
            match self.tokens[p].ty {
                TokenType::LParen => depth += 1,
                TokenType::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(p + 1).map(|t| t.ty),
                            Some(TokenType::Subscript) | Some(TokenType::Superscript)
                        );
                    }
                }
                TokenType::Eof => return false,
                _ => {}
            }
            p += 1;
        }
        false
    }

    /// `(...)` 를 자동크기 괄호 그룹으로 파싱 (#1305). 현재 LParen 전제.
    fn parse_paren_group(&mut self) -> EqNode {
        self.pos += 1; // '(' 소비
        let mut items = Vec::new();
        while !self.at_end() && self.current_type() != TokenType::RParen {
            if self.try_consume_infix_over_atop(&mut items) {
                continue;
            }
            items.push(self.parse_element());
        }
        self.expect(TokenType::RParen); // ')' 소비
        EqNode::Paren {
            left: "(".to_string(),
            right: ")".to_string(),
            body: Box::new(EqNode::Row(items).simplify()),
        }
    }

    /// 단일 토큰 또는 그룹 파싱 (첨자/인자용)
    fn parse_single_or_group(&mut self) -> EqNode {
        if self.at_end() {
            return EqNode::Empty;
        }

        // RBrace는 그룹 종료 마커 — 소비하지 않고 빈 노드 반환
        if self.current_type() == TokenType::RBrace {
            return EqNode::Empty;
        }

        if self.current_type() == TokenType::LBrace {
            return self.parse_group();
        }

        // 단일 토큰
        let ty = self.current_type();
        let val = self.current_value().to_string();
        self.pos += 1;

        match ty {
            TokenType::Command => {
                if let Some(symbol) = lookup_symbol(&val) {
                    EqNode::MathSymbol(symbol.to_string())
                } else if is_function(&val) {
                    EqNode::Function(lookup_function(&val).unwrap_or(&val).to_string())
                } else {
                    // [#1204-B] decoration/구조 명령(bar, sqrt 등)도 단일 인자/body 로
                    // 올 수 있다 (`rm bar {...}`). parse_command 로 위임 — 미지 명령은
                    // parse_command 의 fall-through 가 Text 로 처리하므로 안전.
                    self.parse_command(&val)
                }
            }
            TokenType::Number => EqNode::Number(val),
            TokenType::Text => EqNode::Text(val),
            TokenType::Quoted => EqNode::Quoted(val),
            TokenType::Symbol => EqNode::Symbol(val),
            _ => EqNode::Text(val),
        }
    }

    /// 현재 토큰이 "공백 없이 붙은 관계연산자"인지 (#1304).
    /// HWP 무브레이스 하한 operand 의 연결 구분자 — `sum_k=1`, `lim_x->0`, `1<=k`.
    /// 관계연산자 앞에 공백이 있으면(`x^2 = 4`) false 를 돌려 묶지 않는다.
    fn is_tight_relational(&self) -> bool {
        match self.tokens.get(self.pos) {
            Some(t) => {
                t.ty == TokenType::Symbol
                    && !t.space_before
                    && matches!(
                        t.value.as_str(),
                        "=" | "<" | ">" | "<=" | ">=" | "!=" | "==" | "->"
                    )
            }
            None => false,
        }
    }

    /// 무브레이스 아래첨자/하한 operand 파싱 (#1304).
    /// `원자 (공백없는 관계연산자 원자)*` 패턴으로 하나의 operand 를 묶는다.
    /// `sum_k=1 ^6` 의 하한이 `k=1` 전체가 되도록 한다.
    /// 위첨자(`^`)에는 적용하지 않는다 — `x^2=4` 류 위첨자 등식 보호.
    fn parse_script_operand(&mut self) -> EqNode {
        let first = self.parse_single_or_group();
        if !self.is_tight_relational() {
            return first;
        }
        let mut items = vec![first];
        while self.is_tight_relational() {
            let sym = self.current_value().to_string();
            self.pos += 1;
            // `->` 는 화살표 기호로 변환 (parse_element 와 동일)
            if sym == "->" {
                items.push(EqNode::MathSymbol("→".to_string()));
            } else {
                items.push(EqNode::Symbol(sym));
            }
            items.push(self.parse_single_or_group());
        }
        EqNode::Row(items).simplify()
    }

    /// 첨자(subscript/superscript) 파싱 시도
    /// 한컴 수식에서 함수/기호 뒤에 Thin 공백(`)이 오고 첨자가 따라오는 패턴이 일반적이므로,
    /// Thin 공백 뒤에 첨자가 있으면 공백을 건너뛰고 첨자를 파싱한다.
    fn try_parse_scripts(&mut self, base: EqNode) -> EqNode {
        let mut result = base;
        let mut has_sub = false;
        let mut has_sup = false;
        let mut sub = None;
        let mut sup = None;

        loop {
            if self.at_end() {
                break;
            }
            // Thin 공백(`) 뒤에 첨자가 바로 오는 경우 공백을 건너뛰기
            if self.current_type() == TokenType::Whitespace && self.current_value() == "`" {
                let next_pos = self.pos + 1;
                if next_pos < self.tokens.len() {
                    let next_ty = self.tokens[next_pos].ty;
                    if next_ty == TokenType::Subscript || next_ty == TokenType::Superscript {
                        self.pos += 1; // Thin 공백 건너뛰기
                    }
                }
            }
            let ty = self.current_type();
            if ty == TokenType::Subscript && !has_sub {
                self.pos += 1;
                sub = Some(self.parse_script_operand());
                has_sub = true;
            } else if ty == TokenType::Superscript && !has_sup {
                self.pos += 1;
                sup = Some(self.parse_single_or_group());
                has_sup = true;
            } else {
                break;
            }
        }

        if has_sub && has_sup {
            EqNode::SubSup {
                base: Box::new(result),
                sub: Box::new(sub.unwrap_or(EqNode::Empty)),
                sup: Box::new(sup.unwrap_or(EqNode::Empty)),
            }
        } else if has_sub {
            EqNode::Subscript {
                base: Box::new(result),
                sub: Box::new(sub.unwrap_or(EqNode::Empty)),
            }
        } else if has_sup {
            EqNode::Superscript {
                base: Box::new(result),
                sup: Box::new(sup.unwrap_or(EqNode::Empty)),
            }
        } else {
            result
        }
    }

    /// 분수 파싱: 최상위 OVER 기준으로 분자/분모 분리
    /// LEFT-RIGHT 내부의 OVER는 무시하고 최상위 레벨의 OVER만 분수 분기점으로 사용한다.
    fn parse_fraction(&mut self) -> EqNode {
        // 최상위 OVER 위치를 먼저 찾는다 (brace_depth==0 && lr_depth==0)
        let toplevel_over_pos = {
            let mut brace_depth = 0i32;
            let mut lr_depth = 0i32;
            let mut found = None;
            for i in self.pos..self.tokens.len() {
                let t = &self.tokens[i];
                match t.ty {
                    TokenType::LBrace => brace_depth += 1,
                    TokenType::RBrace => brace_depth -= 1,
                    TokenType::Command => {
                        if Self::cmd_eq(&t.value, "LEFT") {
                            lr_depth += 1;
                        } else if Self::cmd_eq(&t.value, "RIGHT") {
                            lr_depth -= 1;
                        } else if Self::cmd_eq(&t.value, "OVER")
                            && brace_depth == 0
                            && lr_depth == 0
                        {
                            found = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            found
        };

        let over_pos = match toplevel_over_pos {
            Some(p) => p,
            None => return self.parse_expression(), // fallback
        };

        // OVER 앞의 모든 요소를 파싱
        let mut before_nodes = Vec::new();
        while self.pos < over_pos && !self.at_end() {
            before_nodes.push(self.parse_element());
        }
        // OVER 건너뛰기
        if self.current_type() == TokenType::Command && Self::cmd_eq(self.current_value(), "OVER") {
            self.pos += 1;
        }

        // 분자: OVER 바로 앞의 마지막 요소 (그룹 또는 단일 요소)
        let (pre_nodes, numer) = if before_nodes.len() > 1 {
            let numer = before_nodes.pop().unwrap();
            (before_nodes, numer)
        } else {
            (Vec::new(), EqNode::Row(before_nodes).simplify())
        };

        // 분모: OVER 바로 뒤의 첫 번째 요소 (그룹 또는 단일 요소)
        let denom = if !self.at_end() {
            self.parse_element()
        } else {
            EqNode::Empty
        };

        // 분수 뒤 나머지 요소
        let mut after_nodes = Vec::new();
        while !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Command
                && Self::cmd_eq(self.current_value(), "RIGHT")
            {
                break;
            }
            after_nodes.push(self.parse_element());
        }

        let fraction = EqNode::Fraction {
            numer: Box::new(numer),
            denom: Box::new(denom),
        };

        // 앞/뒤 요소와 분수를 Row로 조립
        if pre_nodes.is_empty() && after_nodes.is_empty() {
            fraction
        } else {
            let mut all = pre_nodes;
            all.push(fraction);
            all.extend(after_nodes);
            EqNode::Row(all).simplify()
        }
    }

    /// RBrace까지 분수 파싱
    fn parse_fraction_until_rbrace(&mut self) -> EqNode {
        let mut numer_nodes = Vec::new();
        while !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Command
                && (Self::cmd_eq(self.current_value(), "OVER"))
            {
                self.pos += 1;
                break;
            }
            numer_nodes.push(self.parse_element());
        }

        let mut denom_nodes = Vec::new();
        while !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            denom_nodes.push(self.parse_element());
        }

        EqNode::Fraction {
            numer: Box::new(EqNode::Row(numer_nodes).simplify()),
            denom: Box::new(EqNode::Row(denom_nodes).simplify()),
        }
    }

    /// 제곱근 파싱: SQRT x, SQRT(n) of x
    fn parse_sqrt(&mut self) -> EqNode {
        // LaTeX \sqrt[n]{x} 패턴
        if self.current_type() == TokenType::LBracket {
            self.pos += 1; // [
            let mut index_nodes = Vec::new();
            while !self.at_end() && self.current_type() != TokenType::RBracket {
                index_nodes.push(self.parse_element());
            }
            self.expect(TokenType::RBracket);

            let body = self.parse_single_or_group();
            return EqNode::Sqrt {
                index: Some(Box::new(EqNode::Row(index_nodes).simplify())),
                body: Box::new(body),
            };
        }

        // SQRT(n) of x 패턴 확인 — 소괄호
        if self.current_type() == TokenType::LParen {
            self.pos += 1; // (
            let mut index_nodes = Vec::new();
            while !self.at_end() && self.current_type() != TokenType::RParen {
                index_nodes.push(self.parse_element());
            }
            self.expect(TokenType::RParen); // )

            // 'of' 키워드 건너뛰기
            if self.current_type() == TokenType::Command
                && self.current_value().eq_ignore_ascii_case("of")
            {
                self.pos += 1;
            }

            let body = self.parse_single_or_group();
            return EqNode::Sqrt {
                index: Some(Box::new(EqNode::Row(index_nodes).simplify())),
                body: Box::new(body),
            };
        }

        // SQRT {n} of {x} 패턴 확인 — 중괄호 + of
        if self.current_type() == TokenType::LBrace {
            // 먼저 {n} 뒤에 'of'가 있는지 미리 확인
            let saved_pos = self.pos;
            let brace_end = self.find_matching_brace(self.pos + 1);
            let after_brace = brace_end + 1;
            let has_of = after_brace < self.tokens.len()
                && self.tokens[after_brace].ty == TokenType::Command
                && self.tokens[after_brace].value.eq_ignore_ascii_case("of");

            if has_of {
                // {n} 파싱
                let index = self.parse_group();
                // 'of' 건너뛰기
                if self.current_type() == TokenType::Command
                    && self.current_value().eq_ignore_ascii_case("of")
                {
                    self.pos += 1;
                }
                let body = self.parse_single_or_group();
                return EqNode::Sqrt {
                    index: Some(Box::new(index)),
                    body: Box::new(body),
                };
            }
            // of가 없으면 되돌리고 일반 제곱근으로 처리
            self.pos = saved_pos;
        }

        // 일반 제곱근
        let body = self.parse_single_or_group();
        EqNode::Sqrt {
            index: None,
            body: Box::new(body),
        }
    }

    /// LaTeX 분수 파싱: \frac{numer}{denom}
    fn parse_latex_fraction(&mut self) -> EqNode {
        let numer = self.parse_single_or_group();
        let denom = self.parse_single_or_group();
        EqNode::Fraction {
            numer: Box::new(numer),
            denom: Box::new(denom),
        }
    }

    /// \begin{env}...\end{env} 환경 파싱
    fn parse_latex_environment(&mut self) -> EqNode {
        let env_name = self.read_brace_arg();
        let env_upper = env_name.to_ascii_uppercase();

        match env_upper.as_str() {
            "PMATRIX" => self.parse_latex_env_matrix(MatrixStyle::Paren, &env_name),
            "BMATRIX" => self.parse_latex_env_matrix(MatrixStyle::Bracket, &env_name),
            "VMATRIX" => self.parse_latex_env_matrix(MatrixStyle::Vert, &env_name),
            "BVMATRIX" => self.parse_latex_env_matrix(MatrixStyle::Vert, &env_name),
            "SMALLMATRIX" => self.parse_latex_env_matrix(MatrixStyle::Plain, &env_name),
            "MATRIX" | "ARRAY" => self.parse_latex_env_matrix(MatrixStyle::Plain, &env_name),
            "CASES" => self.parse_latex_env_cases(&env_name),
            "ALIGNED" | "ALIGN" | "EQNARRAY" | "SPLIT" | "GATHER" | "GATHERED" => {
                self.parse_latex_env_eqalign(&env_name)
            }
            _ => EqNode::Empty,
        }
    }

    /// {env_name} 읽기 — 중괄호 안의 텍스트를 반환
    fn read_brace_arg(&mut self) -> String {
        if self.current_type() != TokenType::LBrace {
            return String::new();
        }
        self.pos += 1;
        let mut name = String::new();
        while !self.at_end() && self.current_type() != TokenType::RBrace {
            name.push_str(self.current_value());
            self.pos += 1;
        }
        self.expect(TokenType::RBrace);
        name
    }

    /// \end{env} 의 {env} 인자를 소비하고 건너뛰기
    fn skip_brace_arg(&mut self) {
        if self.current_type() == TokenType::LBrace {
            self.pos += 1;
            while !self.at_end() && self.current_type() != TokenType::RBrace {
                self.pos += 1;
            }
            self.expect(TokenType::RBrace);
        }
    }

    /// \end{env} 도달 여부 확인
    fn at_latex_env_end(&self, env_name: &str) -> bool {
        if self.current_type() != TokenType::Command {
            return false;
        }
        let val = self.current_value();
        if !val.eq_ignore_ascii_case("end") {
            return false;
        }
        let next = self.pos + 1;
        if next >= self.tokens.len() || self.tokens[next].ty != TokenType::LBrace {
            return false;
        }
        let mut i = next + 1;
        let mut name = String::new();
        while i < self.tokens.len() && self.tokens[i].ty != TokenType::RBrace {
            name.push_str(&self.tokens[i].value);
            i += 1;
        }
        name.eq_ignore_ascii_case(env_name)
    }

    /// \end{env}를 소비 (command + {env_name})
    fn consume_latex_env_end(&mut self) {
        if self.current_type() == TokenType::Command
            && self.current_value().eq_ignore_ascii_case("end")
        {
            self.pos += 1;
            self.skip_brace_arg();
        }
    }

    /// LaTeX matrix 환경 파싱: \begin{pmatrix} a & b \\ c & d \end{pmatrix}
    fn parse_latex_env_matrix(&mut self, style: MatrixStyle, env_name: &str) -> EqNode {
        let mut rows: Vec<Vec<EqNode>> = vec![vec![]];
        let mut current_cell = Vec::new();

        while !self.at_end() && !self.at_latex_env_end(env_name) {
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                self.pos += 1;
                if let Some(last_row) = rows.last_mut() {
                    last_row.push(EqNode::Row(current_cell).simplify());
                }
                current_cell = Vec::new();
                rows.push(vec![]);
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                if let Some(last_row) = rows.last_mut() {
                    last_row.push(EqNode::Row(current_cell).simplify());
                }
                current_cell = Vec::new();
                self.pos += 1;
            } else if self.try_consume_infix_over_atop(&mut current_cell) {
                continue;
            } else {
                current_cell.push(self.parse_element());
            }
        }

        if !current_cell.is_empty() || rows.last().map_or(false, |r| !r.is_empty()) {
            if let Some(last_row) = rows.last_mut() {
                last_row.push(EqNode::Row(current_cell).simplify());
            }
        }
        // 빈 마지막 행 제거 (후행 \\ 대응)
        if rows.last().map_or(false, |r| r.is_empty()) {
            rows.pop();
        }

        self.consume_latex_env_end();
        EqNode::Matrix { rows, style }
    }

    /// LaTeX cases 환경 파싱: \begin{cases} expr & cond \\ ... \end{cases}
    fn parse_latex_env_cases(&mut self, env_name: &str) -> EqNode {
        let mut case_rows = Vec::new();
        let mut current_row = Vec::new();

        while !self.at_end() && !self.at_latex_env_end(env_name) {
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                self.pos += 1;
                case_rows.push(EqNode::Row(current_row).simplify());
                current_row = Vec::new();
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                current_row.push(EqNode::Space(SpaceKind::Tab));
                self.pos += 1;
            } else if self.try_consume_infix_over_atop(&mut current_row) {
                continue;
            } else {
                current_row.push(self.parse_element());
            }
        }

        if !current_row.is_empty() {
            case_rows.push(EqNode::Row(current_row).simplify());
        }

        self.consume_latex_env_end();
        EqNode::Cases { rows: case_rows }
    }

    /// LaTeX aligned/align 환경 파싱
    fn parse_latex_env_eqalign(&mut self, env_name: &str) -> EqNode {
        let mut eq_rows: Vec<(EqNode, EqNode)> = Vec::new();
        let mut current_left = Vec::new();
        let mut current_right: Option<Vec<EqNode>> = None;

        while !self.at_end() && !self.at_latex_env_end(env_name) {
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                self.pos += 1;
                let left = EqNode::Row(current_left).simplify();
                let right = current_right
                    .map(|r| EqNode::Row(r).simplify())
                    .unwrap_or(EqNode::Empty);
                eq_rows.push((left, right));
                current_left = Vec::new();
                current_right = None;
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                if current_right.is_none() {
                    current_right = Some(Vec::new());
                }
                self.pos += 1;
            } else {
                let consumed = if let Some(ref mut right) = current_right {
                    self.try_consume_infix_over_atop(right)
                } else {
                    self.try_consume_infix_over_atop(&mut current_left)
                };
                if consumed {
                    continue;
                }
                if let Some(ref mut right) = current_right {
                    right.push(self.parse_element());
                } else {
                    current_left.push(self.parse_element());
                }
            }
        }

        if !current_left.is_empty() || current_right.is_some() {
            let left = EqNode::Row(current_left).simplify();
            let right = current_right
                .map(|r| EqNode::Row(r).simplify())
                .unwrap_or(EqNode::Empty);
            eq_rows.push((left, right));
        }

        self.consume_latex_env_end();
        EqNode::EqAlign { rows: eq_rows }
    }

    /// 큰 연산자 파싱 (적분, 합 등) — 첨자 포함
    fn parse_big_op(&mut self, symbol: String) -> EqNode {
        let mut sub = None;
        let mut sup = None;

        // 첨자 파싱
        loop {
            if self.at_end() {
                break;
            }
            if self.current_type() == TokenType::Subscript && sub.is_none() {
                self.pos += 1;
                sub = Some(Box::new(self.parse_script_operand()));
            } else if self.current_type() == TokenType::Superscript && sup.is_none() {
                self.pos += 1;
                sup = Some(Box::new(self.parse_single_or_group()));
            } else {
                break;
            }
        }

        EqNode::BigOp { symbol, sub, sup }
    }

    /// 극한 파싱
    fn parse_limit(&mut self, is_upper: bool) -> EqNode {
        let mut sub = None;

        if self.current_type() == TokenType::Subscript {
            self.pos += 1;
            sub = Some(Box::new(self.parse_script_operand()));
        }

        EqNode::Limit { is_upper, sub }
    }

    /// 행렬 파싱: MATRIX{a & b # c & d}
    fn parse_matrix(&mut self, style: MatrixStyle) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return EqNode::Empty;
        }

        let end = self.find_matching_brace(self.pos);
        let mut rows: Vec<Vec<EqNode>> = vec![vec![]];
        let mut current_cell = Vec::new();

        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                // 새 행
                if let Some(last_row) = rows.last_mut() {
                    last_row.push(EqNode::Row(current_cell).simplify());
                }
                current_cell = Vec::new();
                rows.push(vec![]);
                self.pos += 1;
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                // 새 셀
                if let Some(last_row) = rows.last_mut() {
                    last_row.push(EqNode::Row(current_cell).simplify());
                }
                current_cell = Vec::new();
                self.pos += 1;
            } else if self.try_consume_infix_over_atop(&mut current_cell) {
                // OVER/ATOP 중위 처리 (#505)
                continue;
            } else {
                current_cell.push(self.parse_element());
            }
        }

        // 마지막 셀 추가
        if !current_cell.is_empty() || rows.last().map_or(false, |r| !r.is_empty()) {
            if let Some(last_row) = rows.last_mut() {
                last_row.push(EqNode::Row(current_cell).simplify());
            }
        }

        self.expect(TokenType::RBrace);

        EqNode::Matrix { rows, style }
    }

    /// 조건식 파싱: CASES{...}
    fn parse_cases(&mut self) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return EqNode::Empty;
        }

        let end = self.find_matching_brace(self.pos);
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                rows.push(EqNode::Row(current_row).simplify());
                current_row = Vec::new();
                self.pos += 1;
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                // && (연속 &): 큰 탭 공간으로 조건 부분 분리
                let mut amp_count = 0;
                while self.pos < end
                    && self.current_type() == TokenType::Whitespace
                    && self.current_value() == "&"
                {
                    amp_count += 1;
                    self.pos += 1;
                }
                for _ in 0..amp_count {
                    current_row.push(EqNode::Space(super::ast::SpaceKind::Tab));
                }
            } else if self.try_consume_infix_over_atop(&mut current_row) {
                // OVER/ATOP 중위 처리 (#505)
                continue;
            } else {
                current_row.push(self.parse_element());
            }
        }

        if !current_row.is_empty() {
            rows.push(EqNode::Row(current_row).simplify());
        }

        self.expect(TokenType::RBrace);

        EqNode::Cases { rows }
    }

    /// 세로 쌓기 파싱
    fn parse_pile(&mut self, align: PileAlign) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return EqNode::Empty;
        }

        let end = self.find_matching_brace(self.pos);
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                rows.push(EqNode::Row(current_row).simplify());
                current_row = Vec::new();
                self.pos += 1;
            } else if self.try_consume_infix_over_atop(&mut current_row) {
                // OVER/ATOP 중위 처리 (#505)
                continue;
            } else {
                current_row.push(self.parse_element());
            }
        }

        if !current_row.is_empty() {
            rows.push(EqNode::Row(current_row).simplify());
        }

        self.expect(TokenType::RBrace);

        EqNode::Pile { rows, align }
    }

    /// EQALIGN 파싱: EQALIGN{row1_left & row1_right # row2_left & row2_right}
    fn parse_eqalign(&mut self) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return EqNode::Empty;
        }

        let end = self.find_matching_brace(self.pos);
        let mut rows: Vec<(EqNode, EqNode)> = Vec::new();
        let mut current_left = Vec::new();
        let mut current_right: Option<Vec<EqNode>> = None;

        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::RBrace {
                break;
            }
            if self.current_type() == TokenType::Whitespace && self.current_value() == "#" {
                // 새 행: 현재 행 완료
                let left = EqNode::Row(current_left).simplify();
                let right = current_right
                    .map(|r| EqNode::Row(r).simplify())
                    .unwrap_or(EqNode::Empty);
                rows.push((left, right));
                current_left = Vec::new();
                current_right = None;
                self.pos += 1;
            } else if self.current_type() == TokenType::Whitespace && self.current_value() == "&" {
                // & 구분: 왼쪽→오른쪽 전환
                // 연속 &&: 큰 탭 공간 (조건 부분 분리용)
                let mut amp_count = 0;
                while self.pos < end
                    && self.current_type() == TokenType::Whitespace
                    && self.current_value() == "&"
                {
                    amp_count += 1;
                    self.pos += 1;
                }
                if current_right.is_none() {
                    current_right = Some(Vec::new());
                    // 연속 && 이면 큰 탭 공간 삽입
                    if amp_count >= 2 {
                        if let Some(ref mut right) = current_right {
                            right.push(EqNode::Space(super::ast::SpaceKind::Tab));
                        }
                    }
                } else if let Some(ref mut right) = current_right {
                    // 이미 오른쪽: 추가 & → 탭 공간
                    for _ in 0..amp_count {
                        right.push(EqNode::Space(super::ast::SpaceKind::Tab));
                    }
                }
            } else {
                // OVER/ATOP 중위 처리 (#505) — 활성 측(right 우선) 의 children 에 적용
                let consumed = if let Some(ref mut right) = current_right {
                    self.try_consume_infix_over_atop(right)
                } else {
                    self.try_consume_infix_over_atop(&mut current_left)
                };
                if consumed {
                    continue;
                }
                if let Some(ref mut right) = current_right {
                    right.push(self.parse_element());
                } else {
                    current_left.push(self.parse_element());
                }
            }
        }

        // 마지막 행 추가
        if !current_left.is_empty() || current_right.is_some() {
            let left = EqNode::Row(current_left).simplify();
            let right = current_right
                .map(|r| EqNode::Row(r).simplify())
                .unwrap_or(EqNode::Empty);
            rows.push((left, right));
        }

        self.expect(TokenType::RBrace);

        EqNode::EqAlign { rows }
    }

    /// LEFT-RIGHT 괄호 파싱
    /// 내부의 OVER는 parse_expression의 중위 연산자 처리로 자동 처리된다.
    fn parse_left_right(&mut self) -> EqNode {
        // LEFT 다음 괄호 문자 읽기
        let left = self.read_bracket_char();

        // RIGHT까지의 내용을 parse_expression으로 파싱
        // parse_expression은 RIGHT를 만나면 자동 중단하고, OVER도 중위 연산자로 처리
        let body = self.parse_expression();

        // RIGHT 건너뛰기
        if self.current_type() == TokenType::Command && Self::cmd_eq(self.current_value(), "RIGHT")
        {
            self.pos += 1;
        }

        // RIGHT 다음 괄호 문자 읽기
        let right = self.read_bracket_char();

        EqNode::Paren {
            left,
            right,
            body: Box::new(body),
        }
    }

    /// 괄호 문자 읽기 (LEFT/RIGHT 뒤)
    fn read_bracket_char(&mut self) -> String {
        if self.at_end() {
            return String::new();
        }

        let ty = self.current_type();
        let val = self.current_value().to_string();

        match ty {
            TokenType::LParen
            | TokenType::RParen
            | TokenType::LBracket
            | TokenType::RBracket
            | TokenType::LBrace
            | TokenType::RBrace => {
                self.pos += 1;
                val
            }
            TokenType::Symbol if val == "|" || val == "." => {
                self.pos += 1;
                if val == "." {
                    String::new() // . = 괄호 생략
                } else {
                    val
                }
            }
            TokenType::Command => {
                // LBRACE, RBRACE 등 명령어
                if let Some(sym) = lookup_symbol(&val) {
                    self.pos += 1;
                    sym.to_string()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    /// RIGHT 위치 찾기 (LEFT-RIGHT 쌍 고려)
    fn find_right_pos(&self) -> usize {
        let mut depth = 1i32;
        let mut pos = self.pos;
        while pos < self.tokens.len() {
            let t = &self.tokens[pos];
            if t.ty == TokenType::Command {
                if Self::cmd_eq(&t.value, "LEFT") {
                    depth += 1;
                } else if Self::cmd_eq(&t.value, "RIGHT") {
                    depth -= 1;
                    if depth == 0 {
                        return pos;
                    }
                }
            }
            pos += 1;
        }
        self.tokens.len()
    }

    /// 범위 내 분수 파싱
    /// OVER 앞/뒤에 중괄호 그룹이 있으면 해당 그룹만 분자/분모로 사용하고
    /// 나머지는 분수 바깥 요소로 처리한다.
    fn parse_fraction_in_range(&mut self, end: usize) -> EqNode {
        // OVER 앞의 모든 요소를 파싱
        let mut before_nodes = Vec::new();
        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::Command
                && Self::cmd_eq(self.current_value(), "OVER")
            {
                self.pos += 1;
                break;
            }
            before_nodes.push(self.parse_element());
        }

        // 분자: OVER 바로 앞의 마지막 요소 (또는 그룹)
        // 나머지 앞 요소들은 분수 앞에 배치
        let (pre_nodes, numer) = if before_nodes.len() > 1 {
            let numer = before_nodes.pop().unwrap();
            (before_nodes, numer)
        } else {
            (Vec::new(), EqNode::Row(before_nodes).simplify())
        };

        // 분모: OVER 바로 뒤의 첫 번째 요소 (또는 그룹)
        let denom = if self.pos < end && !self.at_end() {
            self.parse_element()
        } else {
            EqNode::Empty
        };

        // 분수 뒤 나머지 요소
        let mut after_nodes = Vec::new();
        while self.pos < end && !self.at_end() {
            if self.current_type() == TokenType::Command
                && Self::cmd_eq(self.current_value(), "RIGHT")
            {
                break;
            }
            after_nodes.push(self.parse_element());
        }

        let fraction = EqNode::Fraction {
            numer: Box::new(numer),
            denom: Box::new(denom),
        };

        // 앞/뒤 요소와 분수를 Row로 조립
        if pre_nodes.is_empty() && after_nodes.is_empty() {
            fraction
        } else {
            let mut all = pre_nodes;
            all.push(fraction);
            all.extend(after_nodes);
            EqNode::Row(all).simplify()
        }
    }

    /// COLOR{R,G,B}{body} 파싱
    fn parse_color(&mut self) -> EqNode {
        if !self.expect(TokenType::LBrace) {
            return EqNode::Empty;
        }

        // R, G, B 값 읽기
        let mut rgb = [0u8; 3];
        for i in 0..3 {
            if self.current_type() == TokenType::Number {
                rgb[i] = self.current_value().parse().unwrap_or(0);
                self.pos += 1;
            }
            // 콤마 건너뛰기
            if self.current_type() == TokenType::Symbol && self.current_value() == "," {
                self.pos += 1;
            }
        }
        self.expect(TokenType::RBrace);

        let body = self.parse_single_or_group();

        EqNode::Color {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
            body: Box::new(body),
        }
    }
}

/// 수식 스크립트를 AST로 파싱
pub fn parse(script: &str) -> EqNode {
    let tokens = tokenize(script);
    let mut parser = EqParser::new(tokens);
    parser.parse()
}
