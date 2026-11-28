"""Per-language text normalisation for ASR round-trip compare."""

from __future__ import annotations

import re
import unicodedata
from typing import Iterable


_PUNCT_RE = re.compile(r"[^\w\s]", re.UNICODE)
_WS_RE = re.compile(r"\s+")

# Western number words used for en/nb navigation fragments.
_EN_ONES = {
    0: "zero",
    1: "one",
    2: "two",
    3: "three",
    4: "four",
    5: "five",
    6: "six",
    7: "seven",
    8: "eight",
    9: "nine",
    10: "ten",
    11: "eleven",
    12: "twelve",
    13: "thirteen",
    14: "fourteen",
    15: "fifteen",
    16: "sixteen",
    17: "seventeen",
    18: "eighteen",
    19: "nineteen",
}
_EN_TENS = {
    20: "twenty",
    30: "thirty",
    40: "forty",
    50: "fifty",
    60: "sixty",
    70: "seventy",
    80: "eighty",
    90: "ninety",
}

_NB_ONES = {
    0: "null",
    1: "en",
    2: "to",
    3: "tre",
    4: "fire",
    5: "fem",
    6: "seks",
    7: "syv",
    8: "atte",
    9: "ni",
    10: "ti",
    11: "elleve",
    12: "tolv",
    13: "tretten",
    14: "fjorten",
    15: "femten",
    16: "seksten",
    17: "sytten",
    18: "atten",
    19: "nitten",
}
_NB_TENS = {
    20: "tjue",
    30: "tretti",
    40: "forti",
    50: "femti",
    60: "seksti",
    70: "sytti",
    80: "atti",
    90: "nitti",
}

# Arabic-Indic digits → Western
_ARABIC_INDIC = str.maketrans("٠١٢٣٤٥٦٧٨٩۰۱۲۳۴۵۶۷۸۹", "01234567890123456789")

# Arabic letter normalisations (alef/hamza/yeh/teh marbuta)
_AR_LETTER_MAP = str.maketrans(
    {
        "أ": "ا",
        "إ": "ا",
        "آ": "ا",
        "ٱ": "ا",
        "ى": "ي",
        "ة": "ه",
        "ؤ": "و",
        "ئ": "ي",
    }
)
_AR_TASHKEEL = re.compile(r"[\u0610-\u061A\u064B-\u065F\u0670\u06D6-\u06ED]")
_AR_TATWEEL = "\u0640"


def _en_int_to_words(n: int) -> str:
    if n < 0:
        return str(n)
    if n < 20:
        return _EN_ONES[n]
    if n < 100:
        tens = (n // 10) * 10
        ones = n % 10
        if ones == 0:
            return _EN_TENS[tens]
        return f"{_EN_TENS[tens]} {_EN_ONES[ones]}"
    if n < 1000:
        hundreds = n // 100
        rest = n % 100
        head = f"{_EN_ONES[hundreds]} hundred"
        if rest == 0:
            return head
        return f"{head} {_en_int_to_words(rest)}"
    if n == 1000:
        return "one thousand"
    if n < 2000:
        return f"one thousand {_en_int_to_words(n - 1000)}"
    return str(n)


def _nb_int_to_words(n: int) -> str:
    if n < 0:
        return str(n)
    if n < 20:
        return _NB_ONES[n]
    if n < 100:
        tens = (n // 10) * 10
        ones = n % 10
        if ones == 0:
            return _NB_TENS[tens]
        # Bokmål often concatenates: trettien, but ASR may space — keep spaced form
        return f"{_NB_TENS[tens]} {_NB_ONES[ones]}"
    if n < 1000:
        hundreds = n // 100
        rest = n % 100
        head = f"{_NB_ONES[hundreds]} hundre" if hundreds != 1 else "ett hundre"
        # navigation usually "to hundre"
        if hundreds == 1:
            head = "ett hundre"
        else:
            head = f"{_NB_ONES[hundreds]} hundre"
        if rest == 0:
            return head
        return f"{head} {_nb_int_to_words(rest)}"
    if n == 1000:
        return "tusen"
    return str(n)


def _build_word_to_num(ones: dict[int, str], tens: dict[int, str], hundred: str, thousand: str) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for k, v in ones.items():
        mapping[v] = str(k)
    for k, v in tens.items():
        mapping[v] = str(k)
    mapping[hundred] = "100"
    mapping[thousand] = "1000"
    # also map common two-word hundreds
    for h in range(1, 10):
        mapping[f"{ones[h]} {hundred}"] = str(h * 100)
    mapping[f"one {hundred}"] = "100"
    mapping[f"two {hundred}"] = "200"
    mapping[f"to {hundred}"] = "200"
    mapping[f"ett {hundred}"] = "100"
    return mapping


_EN_WORD_TO_NUM = _build_word_to_num(_EN_ONES, _EN_TENS, "hundred", "thousand")
_EN_WORD_TO_NUM["one thousand"] = "1000"
_NB_WORD_TO_NUM = _build_word_to_num(_NB_ONES, _NB_TENS, "hundre", "tusen")
# Norwegian å-variants without diacritics for ASR compare
_NB_WORD_TO_NUM["atte"] = "8"
_NB_WORD_TO_NUM["atti"] = "80"
_NB_WORD_TO_NUM["forti"] = "40"
_NB_WORD_TO_NUM["en"] = "1"
_NB_WORD_TO_NUM["én"] = "1"


def _numbers_to_words(text: str, lang: str) -> str:
    def repl(m: re.Match[str]) -> str:
        n = int(m.group(0))
        if lang == "nb":
            return _nb_int_to_words(n)
        if lang == "en":
            return _en_int_to_words(n)
        return m.group(0)

    return re.sub(r"\b\d+\b", repl, text)


def _words_to_numbers(text: str, lang: str) -> str:
    mapping = _EN_WORD_TO_NUM if lang == "en" else _NB_WORD_TO_NUM if lang == "nb" else {}
    if not mapping:
        return text
    # longest phrases first
    items = sorted(mapping.items(), key=lambda kv: len(kv[0]), reverse=True)
    out = f" {text} "
    for word, digits in items:
        out = re.sub(rf"(?<!\w){re.escape(word)}(?!\w)", digits, out)
    return _WS_RE.sub(" ", out).strip()


def _normalize_arabic(text: str) -> str:
    text = text.translate(_ARABIC_INDIC)
    text = _AR_TASHKEEL.sub("", text)
    text = text.replace(_AR_TATWEEL, "")
    text = text.translate(_AR_LETTER_MAP)
    return text


def _normalize_cjk(text: str) -> str:
    # full-width → half-width digits/letters where applicable
    out = []
    for ch in text:
        code = ord(ch)
        if 0xFF01 <= code <= 0xFF5E:
            out.append(chr(code - 0xFEE0))
        else:
            out.append(ch)
    return "".join(out)


def _strip_case_and_punct(text: str, lang: str) -> str:
    if lang in ("zh", "ja"):
        text = _normalize_cjk(text)
        text = _PUNCT_RE.sub("", text)
        return text.replace(" ", "")
    text = text.casefold()
    # Norwegian å/ø/æ stay; strip combining marks only for Latin extras beyond nb
    text = _PUNCT_RE.sub(" ", text)
    text = _WS_RE.sub(" ", text).strip()
    # fold å-variants used in ASR for åtte/åtti without requiring exact glyph
    if lang == "nb":
        text = text.replace("å", "a").replace("ø", "o").replace("æ", "ae")
    return text


def normalize_text(text: str, lang_code: str) -> str:
    """Normalise expected text or transcript for equality / WER / CER."""
    lang = lang_code
    text = unicodedata.normalize("NFC", text or "")
    if lang == "ar":
        text = _normalize_arabic(text)
    if lang in ("ru", "uk"):
        text = text.replace("ё", "е").replace("Ё", "Е")
    if lang == "hi":
        text = unicodedata.normalize("NFC", text)

    text = _strip_case_and_punct(text, lang)
    if lang in ("en", "nb"):
        # Map digits → words then words → digits so "30" == "thirty"/"tretti"
        as_words = _numbers_to_words(text, lang)
        as_words = _strip_case_and_punct(as_words, lang)
        as_nums = _words_to_numbers(as_words, lang)
        text = as_nums
    return _WS_RE.sub(" ", text).strip() if lang not in ("zh", "ja") else text


def tokens_for_wer(text: str, lang_code: str) -> list[str]:
    norm = normalize_text(text, lang_code)
    if lang_code in ("zh", "ja"):
        return list(norm)
    if not norm:
        return []
    return norm.split()


def syllable_estimate(text: str, lang_code: str) -> int:
    """Rough syllable / mora / character count for short-clip ASR caveat."""
    norm = normalize_text(text, lang_code)
    if lang_code in ("zh", "ja"):
        return max(1, len(norm))
    if lang_code == "ar":
        return max(1, len(re.findall(r"[\u0600-\u06FF]+", text)) + len(norm.split()))
    # Latin: count vowel groups
    groups = re.findall(r"[aeiouyæøåäöü]+", norm, flags=re.IGNORECASE)
    return max(1, len(groups) if groups else len(norm.split()))


def edit_ops(ref: Iterable[str], hyp: Iterable[str]) -> tuple[list[str], list[str], list[str]]:
    """Return (insertions, deletions, substitutions) via simple Wagner-Fischer backtrace."""
    r = list(ref)
    h = list(hyp)
    n, m = len(r), len(h)
    dp = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1):
        dp[i][0] = i
    for j in range(m + 1):
        dp[0][j] = j
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            cost = 0 if r[i - 1] == h[j - 1] else 1
            dp[i][j] = min(
                dp[i - 1][j] + 1,
                dp[i][j - 1] + 1,
                dp[i - 1][j - 1] + cost,
            )
    ins: list[str] = []
    dele: list[str] = []
    sub: list[str] = []
    i, j = n, m
    while i > 0 or j > 0:
        if i > 0 and j > 0 and r[i - 1] == h[j - 1] and dp[i][j] == dp[i - 1][j - 1]:
            i -= 1
            j -= 1
        elif i > 0 and j > 0 and dp[i][j] == dp[i - 1][j - 1] + 1:
            sub.append(f"{r[i - 1]}->{h[j - 1]}")
            i -= 1
            j -= 1
        elif j > 0 and dp[i][j] == dp[i][j - 1] + 1:
            ins.append(h[j - 1])
            j -= 1
        else:
            dele.append(r[i - 1])
            i -= 1
    return list(reversed(ins)), list(reversed(dele)), list(reversed(sub))


def wer_cer(expected: str, transcript: str, lang_code: str) -> tuple[float, float, dict]:
    ref_toks = tokens_for_wer(expected, lang_code)
    hyp_toks = tokens_for_wer(transcript, lang_code)
    ins, dele, sub = edit_ops(ref_toks, hyp_toks)
    denom = max(1, len(ref_toks))
    errors = len(ins) + len(dele) + len(sub)
    rate = errors / denom
    # For word langs WER==rate; for zh/ja tokens are chars so rate is CER.
    if lang_code in ("zh", "ja"):
        return rate, rate, {"insertions": ins, "deletions": dele, "substitutions": sub}
    # CER on characters of normalised strings
    ref_c = list(normalize_text(expected, lang_code).replace(" ", ""))
    hyp_c = list(normalize_text(transcript, lang_code).replace(" ", ""))
    ci, cd, cs = edit_ops(ref_c, hyp_c)
    cer = (len(ci) + len(cd) + len(cs)) / max(1, len(ref_c))
    return rate, cer, {"insertions": ins, "deletions": dele, "substitutions": sub}
