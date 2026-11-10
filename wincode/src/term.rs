//! Decodificação da saída dos comandos do terminal.
//!
//! Os comandos rodam por pipes, sem TTY, e a saída chega em blocos de até 8 KiB:
//!
//! * uma sequência UTF-8 pode cair na fronteira entre dois blocos, e decodificar
//!   cada bloco isoladamente transformava o acento em "�";
//! * ferramentas antigas (e o próprio `cmd.exe`) escrevem na página de código do
//!   console, que não é UTF-8;
//! * `powershell.exe` redirecionado pode responder em UTF-16.
//!
//! Por isso o decodificador guarda a sobra de cada leitura, decide a codificação
//! uma única vez (BOM ou o padrão de bytes nulos do UTF-16) e, quando o UTF-8
//! não fecha, devolve o byte como Latin-1/Windows-1252 em vez de "�" — é o que
//! mantém "não", "opção" e "ção" legíveis em vez de virarem lixo.

/// Bloco inicial que decide a codificação: bytes nulos em posições pares ou
/// ímpares são o rastro de texto UTF-16 em ASCII.
const MAX_PROBE: usize = 256;

pub(crate) struct Decoder {
    pending: Vec<u8>,
    /// `Some(true)` = big endian; `Some(false)` = little endian; `None` = UTF-8.
    utf16: Option<bool>,
    decided: bool,
}

impl Decoder {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
            utf16: None,
            decided: false,
        }
    }

    /// Decodifica um bloco lido do pipe. Texto incompleto fica guardado para
    /// a próxima leitura (nunca é descartado nem vira "�" à toa).
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        self.take(false)
    }

    /// Devolve o que sobrou no fim do stream (uma sequência cortada, por exemplo).
    pub(crate) fn finish(&mut self) -> String {
        self.take(true)
    }

    fn take(&mut self, end: bool) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        if !self.decided {
            self.utf16 = detect_utf16(&self.pending[..self.pending.len().min(MAX_PROBE)]);
            self.decided = true;
        }
        match self.utf16 {
            Some(big_endian) => self.take_utf16(big_endian, end),
            None => {
                let mut out = self.take_utf8();
                if end {
                    let rest = std::mem::take(&mut self.pending);
                    out.extend(rest.iter().map(|byte| latin1(*byte)));
                }
                out
            }
        }
    }

    fn take_utf16(&mut self, big_endian: bool, end: bool) -> String {
        let mut data = std::mem::take(&mut self.pending);
        let mut start = 0;
        if data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
            start = 2;
        }

        let mut units = Vec::with_capacity(data.len() / 2);
        let mut i = start;
        while i + 1 < data.len() {
            let pair = [data[i], data[i + 1]];
            units.push(if big_endian {
                u16::from_be_bytes(pair)
            } else {
                u16::from_le_bytes(pair)
            });
            i += 2;
        }

        // Metade de um par surrogado pode estar no próximo bloco: guarda de volta.
        if !end
            && units
                .last()
                .is_some_and(|unit| (0xD800..=0xDBFF).contains(unit))
        {
            units.pop();
            i -= 2;
        }

        let out: String = std::char::decode_utf16(units)
            .map(|unit| unit.unwrap_or('\u{FFFD}'))
            .collect();

        if !end && i < data.len() {
            self.pending = data.split_off(i);
        }
        out
    }

    fn take_utf8(&mut self) -> String {
        let data = std::mem::take(&mut self.pending);
        let mut out = String::with_capacity(data.len());
        let mut i = 0;

        while i < data.len() {
            let byte = data[i];
            if byte < 0x80 {
                out.push(byte as char);
                i += 1;
                continue;
            }

            let len = match byte {
                0xC2..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF4 => 4,
                // Byte de continuação solto ou byte inválido em UTF-8.
                _ => 0,
            };

            if len == 0 {
                out.push(latin1(byte));
                i += 1;
                continue;
            }
            if i + len > data.len() {
                // Sequência cortada no fim do bloco: espera a próxima leitura.
                break;
            }
            match std::str::from_utf8(&data[i..i + len]) {
                Ok(text) => {
                    out.push_str(text);
                    i += len;
                }
                Err(_) => {
                    out.push(latin1(byte));
                    i += 1;
                }
            }
        }

        self.pending = data[i..].to_vec();
        out
    }
}

/// Byte isolado como o caractere equivalente em Latin-1/Windows-1252.
fn latin1(byte: u8) -> char {
    match byte {
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        // 0x81, 0x8D, 0x8F, 0x90 e 0x9D não existem no 1252 e ficam como estão.
        _ => byte as char,
    }
}

fn detect_utf16(probe: &[u8]) -> Option<bool> {
    if probe.starts_with(&[0xFF, 0xFE]) {
        return Some(false);
    }
    if probe.starts_with(&[0xFE, 0xFF]) {
        return Some(true);
    }
    if probe.len() < 4 {
        return None;
    }

    let mut even_nul = 0;
    let mut odd_nul = 0;
    for (index, byte) in probe.iter().enumerate() {
        if *byte == 0 {
            if index % 2 == 0 {
                even_nul += 1;
            } else {
                odd_nul += 1;
            }
        }
    }

    // Texto UTF-16 em ASCII tem um byte nulo a cada dois; UTF-8 nunca tem
    // byte nulo em texto comum. A margem de 1/4 evita confundir binário.
    let half = probe.len() / 2;
    if odd_nul >= half / 2 && odd_nul > even_nul {
        return Some(false);
    }
    if even_nul >= half / 2 && even_nul > odd_nul {
        return Some(true);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::Decoder;

    fn decode_chunks(chunks: &[&[u8]]) -> String {
        let mut decoder = Decoder::new();
        let mut out = String::new();
        for chunk in chunks {
            out.push_str(&decoder.push(chunk));
        }
        out.push_str(&decoder.finish());
        out
    }

    #[test]
    fn accents_split_between_reads_are_kept() {
        // "não" em UTF-8: o 'ã' tem dois bytes e o corte cai no meio dele.
        assert_eq!(decode_chunks(&[b"n\xC3", b"\xA3o"]), "não");
    }

    #[test]
    fn latin1_text_does_not_become_replacement_char() {
        // "Serviço" escrito em Windows-1252: ç = 0xE7.
        assert_eq!(decode_chunks(&[b"Servi\xE7o"]), "Serviço");
        assert!(!decode_chunks(&[b"Servi\xE7o"]).contains('\u{FFFD}'));
    }

    #[test]
    fn windows_1252_punctuation_is_mapped() {
        // "…" (0x85) e as aspas curvas vêm de mensagens de ferramentas antigas.
        assert_eq!(decode_chunks(&[b"\x85 \x93ok\x94"]), "… “ok”");
    }

    #[test]
    fn utf16_from_powershell_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "olá".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_chunks(&[&bytes]), "olá");
    }

    #[test]
    fn utf16_without_bom_is_detected_by_nul_pattern() {
        let bytes: Vec<u8> = "ola mundo"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode_chunks(&[&bytes]), "ola mundo");
    }

    #[test]
    fn utf16_surrogate_split_keeps_the_pair() {
        // 😀 = U+1F600, um par surrogado; o bloco termina na metade dele.
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "a😀b".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (first, second) = bytes.split_at(6);
        assert_eq!(decode_chunks(&[first, second]), "a😀b");
    }

    #[test]
    fn cut_sequence_at_end_of_stream_is_flushed() {
        assert_eq!(decode_chunks(&[b"fim\xC3"]), "fimÃ");
    }

    #[test]
    fn plain_ascii_is_untouched() {
        assert_eq!(
            decode_chunks(&[b"linha 1\nlinha 2\r\n"]),
            "linha 1\nlinha 2\r\n"
        );
    }
}
