#!/usr/bin/env python3
"""Lint agent skill packages without importing or executing their contents."""

from __future__ import annotations

import argparse
from bisect import bisect_right
from collections import Counter
from dataclasses import asdict, dataclass, field
from functools import lru_cache
from ipaddress import AddressValueError, IPv4Address, IPv4Network
from itertools import chain
import json
import os
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


DASHES = (("\u2014", "U+2014"), ("\u2013", "U+2013"))
SKILL_NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")
TRIGGER = re.compile(
    r"\b(?:when|whenever)\b|\b(?:use|apply|activate|invoke|load|designed|intended)"
    r" (?:this (?:skill|workflow|tool|mode) )?(?:for|to|if)\b|\btriggers?:", re.I
)
WORKFLOW = re.compile(r"\b(?:workflows?|procedures?|steps|quick ?start|usage|how to)\b", re.I)
VERIFY = re.compile(
    r"\b(?:verif(?:y|ication)|validat(?:e|ion)|checklist|acceptance criteria|"
    r"success criteria|completion(?: criteria)?|done when|confirm that|expected (?:output|result))\b",
    re.I,
)
BOUNDARIES = re.compile(r"\b(?:do not|don['\u2019]t|never|avoid|stop|must not|only if)\b", re.I)

QUOTED = r"\"(?:\\.|[^\"\\])*\"|'(?:''|[^'])*'"
COMMENT = re.compile(rf"(?:^|(?<=[\s:,[{{]))(?:{QUOTED})|(?<!\S)#.*")
YAML_FIELD = re.compile(rf"({QUOTED}|[^\s\[\]{{}},?&*!|>'\"%@`][^:]*?):(?:[ \t]+|$)")
YAML_SEQUENCE = re.compile(r"-(?:[ \t]+|$)")
YAML_COLON = re.compile(r":(?:\s|$)")
YAML_CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x84\x86-\x9f\ufffe\uffff]")
BLOCK_SCALAR = re.compile(r"([|>])([-+]?[1-9]?|[1-9][-+]?)")
YAML_NON_STRING = re.compile(
    r"(?:~|null|true|false|[-+]?(?:0x[\da-f]+|0o[0-7]+|\.inf|"
    r"(?:\d+(?:\.\d*)?|\.\d+)(?:e[-+]?\d+)?)|\.nan)", re.I
)
YAML_ESCAPE = re.compile(r"\\(?:x[\da-fA-F]{2}|u[\da-fA-F]{4}|U[\da-fA-F]{8}|.)")
YAML_ESCAPES = dict(zip('0abtnvfre "/\\N_LP', '\0\a\b\t\n\v\f\r\x1b "/\\\x85\xa0\u2028\u2029'))
FLOW_TOKEN = re.compile(
    rf"\s*(?:{QUOTED}|[,\[\]{{}}]|:(?=[\s,\[\]{{}}]|$)|(?<=[\"']):|"
    r"(?:(?!:(?=[\s,\[\]{}]|$))[^,\[\]{}])+)"
)


class FrontmatterError(Exception):
    def __init__(self, line):
        self.line = line


def yaml_content(text):
    for match in COMMENT.finditer(text):
        if match[0].startswith("#"):
            return text[:match.start()].rstrip()
    return text.rstrip()


def yaml_scalar(text, line):
    if not text or YAML_NON_STRING.fullmatch(text):
        return None
    if text[0] == "'" and re.fullmatch(QUOTED, text):
        return text[1:-1].replace("''", "'")
    if text[0] == '"' and re.fullmatch(QUOTED, text):
        def decode(match):
            escape = match[0][1:]
            try:
                if len(escape) > 1:
                    return chr(int(escape[1:], 16))
                return YAML_ESCAPES[escape]
            except (KeyError, ValueError):
                raise FrontmatterError(line) from None

        return YAML_ESCAPE.sub(decode, text[1:-1])
    if (text[0] in "&*!|>'\"%@`[]{} ," or text in ("-", "?", ":")
            or text.startswith(("- ", "? ")) or YAML_COLON.search(text)):
        raise FrontmatterError(line)
    return text


class FlowYaml:
    def __init__(self, raw, line):
        self.tokens = iter(match[0].strip() for match in FLOW_TOKEN.finditer(raw))
        self.line = line
        self.advance()

    def advance(self):
        self.token = next(self.tokens, None)

    def value(self, depth):
        token = self.token
        self.advance()
        if depth > 48 or token in (None, ":", ",", "]", "}"):
            raise FrontmatterError(self.line)
        if token not in ("[", "{"):
            return yaml_scalar(token, self.line)

        closing = "]" if token == "[" else "}"
        result = [] if token == "[" else {}
        while self.token != closing:
            entry = self.value(depth + 1)
            if token == "[":
                result.append(entry)
            else:
                if not isinstance(entry, str) or entry in result or self.token != ":":
                    raise FrontmatterError(self.line)
                self.advance()
                result[entry] = None if self.token in (",", "}") else self.value(depth + 1)
            if self.token == closing:
                break
            if self.token != ",":
                raise FrontmatterError(self.line)
            self.advance()
        self.advance()
        return result


def yaml_inline(raw, line):
    if raw.startswith(("[", "{")):
        flow = FlowYaml(raw, line)
        value = flow.value(0)
        if flow.token is not None:
            raise FrontmatterError(line)
        return value
    return yaml_scalar(raw, line)


class Frontmatter:
    """Read common YAML frontmatter data without tags, aliases or directives."""

    def __init__(self, lines):
        self.lines = list(lines)
        self.field_lines = {}

    def next_content(self, index):
        while index < len(self.lines):
            raw = self.lines[index]
            if raw.lstrip(" ").startswith("\t"):
                raise FrontmatterError(index + 2)
            if raw.strip() and not raw.lstrip().startswith("#"):
                break
            index += 1
        return index

    def indent(self, index):
        if index == len(self.lines):
            return -1
        return len(self.lines[index]) - len(self.lines[index].lstrip(" "))

    def value(self, raw, index, indent, depth, indentless_sequence=False):
        line = index + 2
        block = BLOCK_SCALAR.fullmatch(raw)
        if block:
            start = index + 1
            end = start
            width = re.search(r"[1-9]", block[2])
            width = indent + int(width[0]) if width else None
            while end < len(self.lines):
                if self.lines[end].strip():
                    if self.indent(end) <= indent:
                        break
                    if width is None:
                        width = self.indent(end)
                    if self.indent(end) < width:
                        raise FrontmatterError(end + 2)
                end += 1
            separator = "\n" if block[1] == "|" else " "
            return separator.join(text[width:] for text in self.lines[start:end]), end

        next_index = self.next_content(index + 1)
        next_indent = self.indent(next_index)
        if not raw:
            if next_indent > indent or (indentless_sequence and next_indent == indent):
                child = yaml_content(self.lines[next_index][next_indent:])
                if YAML_SEQUENCE.match(child) or (next_indent > indent and YAML_FIELD.match(child)):
                    return self.block(next_index, next_indent, depth + 1)
                if next_indent > indent:
                    return self.value(child, next_index, indent, depth)
            return None, next_index
        is_flow = raw.startswith(("[", "{"))
        flow_end = "]" if raw.startswith("[") else "}"
        if next_indent > indent or is_flow:
            parts = [raw]
            while self.indent(next_index) > indent:
                extra = yaml_content(self.lines[next_index].lstrip())
                if not raw.startswith(("[", "{", '"', "'")) and (
                    YAML_COLON.search(extra) or YAML_SEQUENCE.match(extra)
                ):
                    raise FrontmatterError(next_index + 2)
                parts.append(extra)
                next_index = self.next_content(next_index + 1)
            if is_flow and not parts[-1].endswith(flow_end) and self.indent(next_index) == indent:
                closing = yaml_content(self.lines[next_index].lstrip())
                if closing == flow_end:
                    parts.append(closing)
                    next_index = self.next_content(next_index + 1)
            raw = " ".join(parts)
        return yaml_inline(raw, line), next_index

    def block(self, index, indent, depth=0):
        if depth > 48:
            raise FrontmatterError(index + 2)
        sequence = bool(YAML_SEQUENCE.match(self.lines[index][indent:]))
        result = [] if sequence else {}
        while self.indent(index) == indent:
            line = index + 2
            raw = yaml_content(self.lines[index][indent:])
            if sequence:
                item = YAML_SEQUENCE.match(raw)
                if not item:
                    break
                entry = raw[item.end():]
                if YAML_FIELD.match(entry) or YAML_SEQUENCE.match(entry):
                    child_indent = indent + item.end()
                    self.lines[index] = " " * child_indent + entry
                    value, index = self.block(index, child_indent, depth + 1)
                else:
                    value, index = self.value(entry, index, indent, depth)
                result.append(value)
            else:
                item = YAML_FIELD.match(raw)
                if not item:
                    raise FrontmatterError(line)
                key = yaml_scalar(item[1].rstrip(), line)
                if not isinstance(key, str) or key in result:
                    raise FrontmatterError(line)
                if depth == 0:
                    self.field_lines[key] = line
                result[key], index = self.value(raw[item.end():], index, indent, depth, indentless_sequence=True)
        return result, index

    def parse(self):
        for index, line in enumerate(self.lines, 2):
            if YAML_CONTROL.search(line):
                raise FrontmatterError(index)
        index = self.next_content(0)
        if index == len(self.lines):
            return {}
        indent = self.indent(index)
        raw = yaml_content(self.lines[index].lstrip())
        if raw.startswith("{"):
            result, end = self.value(raw, index, indent, 0)
        else:
            result, end = self.block(index, indent)
        if not isinstance(result, dict) or end != len(self.lines):
            raise FrontmatterError(end + 2 if end < len(self.lines) else index + 2)
        return result


SECRET_FORMAT = re.compile(
    r"\b(?:sk-(?:proj-|ant-|svcacct-)?[A-Za-z0-9_-]{16,}|"
    r"gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|"
    r"glpat-[A-Za-z0-9_-]{18,}|xox[baprs]-[A-Za-z0-9-]{12,}|"
    r"(?:AKIA|ASIA)[A-Z0-9]{16}|AIza[A-Za-z0-9_-]{35}|"
    r"[rs]k_live_[A-Za-z0-9]{16,}|eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})\b"
    r"|-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----"
)
CREDENTIAL = re.compile(
    r"(?<![\w-])(?:[\w]+[_-])?"
    r"(?:api[_-]?key|(?:access|refresh|auth|bearer|session)[_-]?token|token|"
    r"(?:client|api)[_-]?secret|secret(?:[_-]?key)?|pass(?:word|wd|phrase)|"
    r"private[_-]?key|aws[_-]?secret[_-]?access[_-]?key)[\"']?\s*[:=]\s*"
    r'''(?:"([^"\r\n]+)"|'([^'\r\n]+)'|([^\s"'`,;<>\[\]{}()#]+))''',
    re.I,
)
BEARER = re.compile(r"\bauthorization\s*:\s*bearer\s+([A-Za-z0-9._~+/-]{8,}={0,2})", re.I)
URI_CREDENTIAL = re.compile(r"\b[a-z][a-z0-9+.-]*://[^\s/:@]+:([^\s/@]+)@", re.I)
PLACEHOLDER = re.compile(
    r"(?:\.{2,}|[x*]{3,}|None|null|true|false|changeme|replace[-_]?me|"
    r"(?:example|dummy|placeholder|redacted|your|insert)(?:[-_ ][\w -]+)?)", re.I
)
ENV_REFERENCE = re.compile(r"(?:(?:process|import\.meta)\.env\.|os\.(?:environ|getenv)\b|"
                           r"(?:getenv|env|config|settings|secrets)\.|getenv\b|ENV\[)")
VARIABLE_REFERENCE = re.compile(r"\$[A-Za-z_][\w]*|%[A-Za-z_][\w]*%|\{[\w.-]+\}")
IPV4 = re.compile(r"(?<![\w.])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![\w]|\.[0-9])")
# RFC 1918 network addresses (32-bit integers) and prefix lengths.
PRIVATE_NETWORKS = tuple(map(IPv4Network, (
    (0x0A000000, 8),
    (0xAC100000, 12),
    (0xC0A80000, 16),
)))


def literal_credential(value):
    return bool(value and not (
        value.startswith(("${", "$(", "<", "{{", "[`", "`"))
        or PLACEHOLDER.fullmatch(value)
        or ENV_REFERENCE.match(value)
        or VARIABLE_REFERENCE.fullmatch(value)
    ))


def has_secret(line):
    if SECRET_FORMAT.search(line):
        return True
    for pattern in (CREDENTIAL, BEARER, URI_CREDENTIAL):
        for match in pattern.finditer(line):
            value = next(group for group in match.groups() if group is not None)
            if literal_credential(value) and (len(value) >= 4 or pattern is URI_CREDENTIAL):
                return True
    return False


def has_private_ip(line):
    for match in IPV4.finditer(line):
        try:
            address = IPv4Address(match[0])
        except AddressValueError:
            continue
        if any(address in network for network in PRIVATE_NETWORKS):
            return True
    return False


@lru_cache(maxsize=512)
def display_path(path):
    raw = str(path)
    if has_secret(raw) or has_secret(unquote(raw)):
        return "<redacted path>"
    return "".join(char if char.isprintable() and char not in "\u2013\u2014"
                   else ascii(char)[1:-1] for char in raw)


REMOTE_PIPE = re.compile(
    r"(?<![\w.-])(?:curl|wget)\b[^|;\n]*\|\s*(?:sudo\s+)?(?:[\w./-]*/)?"
    r"(?:sh|bash|dash|zsh|ksh|fish)\b", re.I
)
PAYMENT = re.compile(r"(?<![\w.-])colab\s+pay\b")
REMOVE = re.compile(r"(?<![\w.-])rm[ \t]+([^;|&#`\n]+)")
SHELL_ARG = re.compile(r'''"[^"\n]*"|'[^'\n]*'|[^\s]+''')
TEMP_TARGET = re.compile(r"/(?:tmp|var/tmp)/(?:[\w.-]+/)*[\w.-]+/?$")
REVIEW = re.compile(r"\b(?:review|inspect|read|audit)\b", re.I)
REMOTE_SUBJECT = re.compile(r"\b(?:script|source|payload|download(?:ed)?|installer)\b", re.I)
APPROVAL = re.compile(r"\b(?:explicit (?:user )?(?:approval|confirmation|consent|permission)|"
                      r"(?:ask|prompt|wait) (?:the user )?for (?:approval|confirmation)|"
                      r"(?:ask|require) (?:the )?user to (?:approve|confirm)|"
                      r"(?:user|operator|human) (?:approval|confirmation|consent))\b", re.I)
ORDER = re.compile(r"\b(?:before|first|until|after|without|unless|prior to)\b", re.I)
DENIAL = re.compile(r"\b(?:never|avoid|do not|don['\u2019]t|must not|forbidden|prohibited|deny)\b", re.I)
UNSAFE_GUARD = re.compile(r"\b(?:skip|bypass|waive|no need for|do not|never)\s+"
                          r"(?:the |any |an? )?(?:review|inspect|read|audit|ask|wait|require|"
                          r"approval|confirmation|consent|permission)\b", re.I)
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
HEADING = re.compile(r"^ {0,3}#{1,6}\s+(.+)")


def has_unsafe_delete(line):
    for match in REMOVE.finditer(line):
        options, targets = set(), []
        end_options = False
        for arg in SHELL_ARG.findall(match[1]):
            if end_options or not arg.startswith("-"):
                targets.append(arg.strip("\"'"))
            elif arg == "--":
                end_options = True
            elif arg.startswith("--"):
                options.add(arg)
            else:
                options.update(arg[1:])
        recursive = bool(options & {"r", "R", "--recursive"})
        forced = bool(options & {"f", "--force"})
        if recursive and any(
            target in {"/", "/*", "~", "~/", "~/*", "$HOME", "${HOME}"} or forced and (
                not TEMP_TARGET.fullmatch(target) or {".", ".."}.intersection(target.split("/"))
            ) for target in targets if target
        ):
            return True
    return False


def guarded(line, previous, command, remote=False):
    if UNSAFE_GUARD.search(line):
        return False
    match = command.search(line)
    prefix = re.split(r"\b(?:but|however|instead)\b|[.;]", line[:match.start()], flags=re.I)[-1]
    suffix = line[match.end():]
    if DENIAL.search(prefix) and not re.search(r"\b(?:unless|except|without)\b", suffix, re.I):
        return True
    context = " ".join([*(text for text in previous if not command.search(text)), line])
    if not ORDER.search(context) or UNSAFE_GUARD.search(context):
        return False
    if remote:
        return bool(REVIEW.search(context) and REMOTE_SUBJECT.search(context))
    return bool(APPROVAL.search(context))


def command_lines(lines):
    """Join shell continuations while retaining the first physical line."""
    first = 1
    parts = []
    for number, line in enumerate(lines, 1):
        stripped = line.rstrip()
        if not parts:
            first = number
        if stripped.endswith("\\"):
            parts.append(stripped[:-1])
        else:
            yield first, " ".join([*parts, line]) if parts else line
            parts.clear()
    if parts:
        yield first, " ".join(parts)


def markdown_lines(lines):
    """Keep prose line numbers while masking fenced code and HTML comments."""
    marker = ""
    in_comment = False
    in_indented_code = False
    blank = True
    for line in lines:
        fence = FENCE.match(line)
        if marker:
            if fence and fence[1][0] == marker[0] and len(fence[1]) >= len(marker) and not fence[2].strip():
                marker = ""
            yield ""
            continue
        if fence and not in_comment:
            marker = fence[1]
            yield ""
            continue
        in_indented_code = not in_comment and (
            line.startswith(("    ", "\t")) and (blank or in_indented_code)
            or not line.strip() and in_indented_code
        )
        blank = not line.strip()
        if in_indented_code:
            yield ""
            continue
        pieces = []
        while line:
            if in_comment:
                _, end, line = line.partition("-->")
                in_comment = not end
                continue
            before, start, after = line.partition("<!--")
            pieces.append(before)
            in_comment = bool(start)
            line = after
        yield " ".join(pieces)


CODE_SPAN = re.compile(r"(?<![\\`])(`+)(?!`)[\s\S]*?(?<!`)\1(?!`)")
LINK_LABEL = r"\[(?:\\[^\n]|[^\[\]\\\n]|\[[^\[\]\n]*\])*\]"
LINK_CHAR = r"(?:\\[^\n]|[^\s()\\<>])"
LINK_GROUP = rf"\((?:{LINK_CHAR}|\({LINK_CHAR}*\))*\)"
LINK_DEST = rf"(?P<target><[^<>\n]*>|(?:{LINK_CHAR}|{LINK_GROUP})*)"
LINK_TITLE = r'''(?:"[^"\n]*"|'[^'\n]*'|\([^()\n]*\))'''
INLINE_LINK = re.compile(rf"(?<!\\){LINK_LABEL}\([\s]*{LINK_DEST}(?:\s+{LINK_TITLE})?\s*\)")
REFERENCE_LINK = re.compile(rf"^ {{0,3}}{LINK_LABEL}:[ \t\n]*{LINK_DEST}(?=[ \t\n]|$)", re.M)
ESCAPED_PUNCTUATION = re.compile(r"\\([!\"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])")


def link_targets(lines, start):
    content = "\n".join(lines)
    if "[" not in content:
        return
    prose = CODE_SPAN.sub(lambda match: re.sub(r"[^\n]", " ", match[0]), content)
    offsets = [0, *(match.end() for match in re.finditer("\n", prose))]
    for pattern in (INLINE_LINK, REFERENCE_LINK):
        for match in pattern.finditer(prose):
            line = start + bisect_right(offsets, match.start())
            yield line, ESCAPED_PUNCTUATION.sub(r"\1", match["target"].strip("<>"))


@dataclass
class Finding:
    severity: str
    code: str
    file: str
    line: int | None
    message: str


@dataclass
class Result:
    path: str
    skill: bool = False
    lines: int | None = None
    estimated_tokens: int | None = None
    findings: list[Finding] = field(default_factory=list)

    def add(self, severity, code, file, line, message):
        self.findings.append(Finding(severity, code, display_path(file), line, message))


def check_frontmatter(path, lines, result):
    if not lines or lines[0].rstrip() != "---":
        result.add("error", "frontmatter-missing", path, 1, "Start SKILL.md with YAML frontmatter delimited by ---.")
        return 0
    end = next((index for index, line in enumerate(lines[1:], 1) if line.rstrip() == "---"), None)
    if end is None:
        result.add("error", "frontmatter-unclosed", path, 1, "Close YAML frontmatter with --- on its own line.")
        return 0

    parser = Frontmatter(lines[1:end])
    try:
        fields = parser.parse()
    except FrontmatterError as error:
        result.add("error", "frontmatter-yaml", path, error.line, "Expected a valid skill YAML mapping with unique keys.")
        return end + 1

    name = fields.get("name")
    name_line = parser.field_lines.get("name", 1)
    if not isinstance(name, str) or not SKILL_NAME.fullmatch(name):
        result.add("error", "name-format", path, name_line, "Use a kebab-case name with lowercase ASCII letters or digits.")
    if isinstance(name, str) and name != path.parent.name:
        result.add("error", "name-mismatch", path, name_line, "The name must match the skill directory name.")

    description = fields.get("description")
    line = parser.field_lines.get("description", 1)
    if not isinstance(description, str) or not description.strip():
        result.add("error", "description-missing", path, line, "Provide a non-empty description string.")
    else:
        description = " ".join(description.split())
        if len(description) > 1024:
            result.add("error", "description-length", path, line, "Keep the description within 1,024 characters.")
        if not TRIGGER.search(description):
            result.add("warning", "description-trigger", path, line, "State when to use the skill, with concrete trigger language.")
    return end + 1


def check_structure(path, prose, start, result):
    headings = [match[1] for line in prose if (match := HEADING.match(line))]
    text = "\n".join(prose)
    steps = re.findall(r"(?m)^\s{0,3}\d+[.)]\s+\S", text)
    if not any(WORKFLOW.search(heading) for heading in headings) and len(steps) < 2:
        result.add("warning", "workflow-missing", path, start, "Include a workflow heading or numbered steps.")
    if not VERIFY.search(text):
        result.add("warning", "verification-missing", path, start, "Describe verification or completion criteria.")
    if not BOUNDARIES.search(text):
        result.add("warning", "guardrails-missing", path, start, "State explicit negative boundaries or stop conditions.")


def check_text(path, lines, result, public):
    for number, line in enumerate(lines, 1):
        for char, codepoint in DASHES:
            if char in line:
                result.add("error", "unicode-dash", path, number, f"Replace {codepoint} with ASCII punctuation.")
        if has_secret(line):
            result.add("error", "hardcoded-secret", path, number, "Possible hardcoded credential. Replace it with a secret reference.")
        if public and has_private_ip(line):
            result.add("error", "private-ip", path, number, "Private RFC 1918 IPv4 address in a public skill.")

    nearby = []
    for number, line in command_lines(lines):
        if HEADING.match(line):
            nearby.clear()
        is_fence = FENCE.match(line)
        if is_fence or not line.strip():
            nearby = [(n, text) for n, text in nearby if number - n <= 3]
            continue
        context = [text for n, text in nearby if number - n <= 3]
        if REMOTE_PIPE.search(line) and not guarded(line, context, REMOTE_PIPE, remote=True):
            result.add("error", "unsafe-shell-pipe", path, number, "Download and review the script before running it.")
        if PAYMENT.search(line) and not guarded(line, context, PAYMENT):
            result.add("error", "unsafe-payment", path, number, "Require explicit user approval before running colab pay.")
        if has_unsafe_delete(line) and not guarded(line, context, REMOVE):
            result.add("error", "unsafe-delete", path, number, "Scope recursive forced deletion and require explicit confirmation.")
        nearby = [(n, text) for n, text in nearby if number - n < 3]
        nearby.append((number, line))


def check_links(path, root, prose, start, result):
    for number, target in link_targets(prose, start):
        if not target or target.startswith(("#", "?", "/")):
            continue
        try:
            link = urlsplit(target)
            if link.scheme or link.netloc or not link.path:
                continue
            destination = (path.parent / unquote(link.path)).resolve()
            if destination != root and root not in destination.parents:
                result.add("error", "link-outside-skill", path, number, "Relative Markdown link leaves the skill directory.")
            elif not (destination.is_file() or destination.is_dir()):
                result.add("error", "broken-link", path, number, "Relative Markdown link target does not exist.")
        except (OSError, ValueError, RuntimeError):
            result.add("error", "broken-link", path, number, "Relative Markdown link target cannot be resolved.")


def read_text(path, result, required):
    try:
        if not path.is_file():
            raise OSError
        raw = path.read_bytes()
    except OSError:
        result.add("error", "read-error", path, None, "Could not read a regular skill file.")
        return None
    try:
        if b"\0" in raw:
            raise ValueError
        return raw.decode("utf-8-sig").replace("\r\n", "\n").replace("\r", "\n")
    except ValueError:
        if required or path.suffix.lower() in (".md", ".markdown"):
            result.add("error", "file-encoding", path, None, "Expected a UTF-8 Markdown file.")
        return None


def skill_files(directory, result):
    def on_error(error):
        result.add("error", "read-error", error.filename or directory, None, "Could not list skill directory contents.")

    for folder, directories, files in os.walk(directory, onerror=on_error):
        directories[:] = sorted(name for name in directories if name != ".git")
        for name in sorted(files):
            path = Path(folder) / name
            if not path.is_symlink() and path.is_file():
                yield path


def lint_skill(entry, strict, public):
    result = Result(display_path(entry), skill=True)
    try:
        root = entry.parent.resolve()
        if root not in entry.resolve().parents:
            result.add("error", "entry-outside-skill", entry, None, "SKILL.md resolves outside the skill directory.")
            return result
    except (OSError, RuntimeError):
        result.add("error", "read-error", entry, None, "Could not resolve SKILL.md.")
        return result

    for path in chain((entry,), (path for path in skill_files(entry.parent, result) if path != entry)):
        text = read_text(path, result, required=path == entry)
        if text is None:
            continue
        lines = text.split("\n")
        if lines[-1] == "":
            lines.pop()
        body = 0
        if path == entry:
            result.lines = len(lines)
            result.estimated_tokens = (len(text) + 3) // 4
            body = check_frontmatter(path, lines, result)
            if strict and result.estimated_tokens > 6000:
                result.add("error", "size-budget", path, 1, "Exceeds 6,000 estimated tokens. Move detail to references/ or scripts/.")
            elif result.estimated_tokens > 3500 or result.lines > 350:
                result.add("warning", "size-budget", path, 1, "Exceeds 3,500 estimated tokens or 350 lines. Move detail to references/ or scripts/.")
        check_text(path, lines, result, public)
        if path == entry or path.suffix.lower() in (".md", ".markdown"):
            prose = list(markdown_lines(lines[body:]))
            check_links(path, root, prose, body, result)
            if path == entry:
                check_structure(path, prose, min(body + 1, max(1, len(lines))), result)
    result.findings.sort(key=lambda item: (item.file, item.line or 0, item.code, item.message))
    return result


def targets(path):
    if path.is_dir():
        entry = path / "SKILL.md"
        if entry.exists() or entry.is_symlink():
            return [entry]
        children = sorted(
            child / "SKILL.md" for child in path.iterdir()
            if child.is_dir() and ((child / "SKILL.md").exists() or (child / "SKILL.md").is_symlink())
        )
        if children:
            return children
    elif path.name == "SKILL.md" and path.is_file():
        return [path]
    return []


class ArgumentParser(argparse.ArgumentParser):
    def error(self, message):
        self.exit(1, "skill-lint: invalid arguments. See --help.\n")


def main(argv=None):
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", metavar="PATH", help="skill directory, SKILL.md, or direct parent of skills")
    parser.add_argument("--json", action="store_true", help="emit a JSON report")
    parser.add_argument("--strict", action="store_true", help="treat more than 6,000 estimated tokens as an error")
    parser.add_argument("--public", action="store_true", help="flag private RFC 1918 IPv4 addresses")
    parser.epilog = "Frontmatter supports common YAML mappings, sequences and scalars."
    args = parser.parse_intermixed_args(argv)

    results = []
    seen = set()
    for raw in args.paths:
        path = Path(os.path.abspath(raw))
        try:
            entries = targets(path)
        except (OSError, ValueError):
            entries = []
        if not entries and path not in seen:
            seen.add(path)
            result = Result(display_path(path))
            result.add("error", "invalid-path", path, None, "Expected a readable SKILL.md or a directory containing skills.")
            results.append(result)
        for entry in entries:
            if entry not in seen:
                seen.add(entry)
                results.append(lint_skill(entry, args.strict, args.public))

    totals = Counter(finding.severity for result in results for finding in result.findings)
    summary = {"skills": sum(result.skill for result in results),
               "errors": totals["error"], "warnings": totals["warning"]}
    if args.json:
        print(json.dumps({"summary": summary, "results": [asdict(result) for result in results]}, indent=2))
    else:
        for result in results:
            for finding in result.findings:
                location = f"{finding.file}:{finding.line}" if finding.line is not None else finding.file
                print(f"{location}: {finding.severity} [{finding.code}] {finding.message}")
        print(f"{summary['skills']} skill(s), {summary['errors']} error(s), {summary['warnings']} warning(s)")
    return int(bool(totals["error"]))


if __name__ == "__main__":
    raise SystemExit(main())
