use super::{Function, Line, Syntax};

const KEYWORDS: &[&str] = &[
    "if", "else", "while", "for", "return", "break", "struct", "sizeof",
];
const TYPES: &[&str] = &["void", "int", "char", "uint", "bool", "byte", "FILE"];
const CONSTANTS: &[&str] = &["true", "false", "NULL"];

// TODO: the Ghidra adapter should hand over each function's lines, already marked, instead
// of this mock. A function reads as its plate comment, then its C.
pub fn functions() -> Vec<Function> {
    FUNCTIONS
        .iter()
        .map(|&(name, note, code)| {
            let note = note.map(|note| format!("/* {note} */"));
            let lines = note.iter().map(String::as_str).chain(code.lines());
            Function {
                name,
                signature: usize::from(note.is_some()),
                lines: lines.map(lex).collect(),
            }
        })
        .collect()
}

// Marks a line the way Ghidra's decompiler does; what it leaves unmarked is plain.
fn lex(text: &str) -> Line {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut after_struct = false;
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let syntax = match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = text[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| at + 2 + end + 2);
                Some(Syntax::Comment)
            }
            quote @ (b'"' | b'\'') => {
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                at = (at + 1).min(bytes.len());
                Some(Syntax::String)
            }
            b'0'..=b'9' => {
                at += bytes[at..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric())
                    .count();
                Some(Syntax::Constant)
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                at += bytes[at..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_')
                    .count();
                let word = &text[start..at];
                let syntax = identifier(word, bytes.get(at) == Some(&b'('), after_struct);
                after_struct = word == "struct";
                syntax
            }
            _ => {
                at += 1;
                None
            }
        };
        spans.extend(syntax.map(|syntax| (start..at, syntax)));
    }
    Line {
        text: text.to_owned().into(),
        spans,
    }
}

// What an identifier names. A call names one of ours or a library's, even in capitals,
// as FUN_00413b10 does.
fn identifier(word: &str, called: bool, after_struct: bool) -> Option<Syntax> {
    if KEYWORDS.contains(&word) {
        Some(Syntax::Keyword)
    } else if CONSTANTS.contains(&word) {
        Some(Syntax::Constant)
    } else if called && FUNCTIONS.iter().any(|&(name, ..)| name == word) {
        Some(Syntax::Function)
    } else if called {
        Some(Syntax::External)
    } else if TYPES.contains(&word) || word.ends_with("_t") || after_struct {
        Some(Syntax::Type)
    } else if word
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        Some(Syntax::Constant)
    } else {
        None
    }
}

// The Electron build's decompiled functions: name, plate comment, C.
const FUNCTIONS: &[(&str, Option<&str>, &str)] = &[
    (
        "main",
        None,
        r#"int main(int argc, char **argv)
{
  int server;
  int client;
  struct sockaddr_in addr;
  socklen_t addr_len;

  load_config("/etc/config.json");
  server = socket(AF_INET, SOCK_STREAM, 0);
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(g_config.port);
  bind(server, (struct sockaddr *)&addr, sizeof(addr));
  listen(server, 16);
  FUN_00415a20();
  while (true) {
    addr_len = sizeof(addr);
    client = accept(server, (struct sockaddr *)&addr, &addr_len);
    if (-1 < client) {
      handle_client(client);
    }
  }
}"#,
    ),
    (
        "handle_client",
        Some("One connection at a time, served inline."),
        r#"void handle_client(int fd)
{
  conn_t conn;
  char buf[1024];

  memset(&conn, 0, sizeof(conn));
  conn.fd = fd;
  if (parse_request(&conn, buf, sizeof(buf)) < 0) {
    close(fd);
    return;
  }
  if (strncmp(conn.path, "/api/", 5) == 0 && !check_session(&conn)) {
    send_response(&conn, 401, "Unauthorized");
  }
  else {
    serve_file(&conn, conn.path);
  }
  close(fd);
}"#,
    ),
    (
        "parse_request",
        Some("Request line, then headers until the blank line."),
        r#"int parse_request(conn_t *conn, char *buf, int len)
{
  char *line;
  int content_length;

  line = read_line(conn, buf, len);
  if (line == NULL) {
    return -1;
  }
  if (sscanf(line, "%7s %255s HTTP/1.%*d", conn->method, conn->path) != 2) {
    send_response(conn, 400, "Bad Request");
    return -1;
  }
  url_decode(conn->path);

  content_length = 0;
  while ((line = read_line(conn, buf, len)) != NULL && *line != '\0') {
    if (strncasecmp(line, "Content-Length:", 15) == 0) {
      content_length = atoi(line + 15);
    }
    else if (strncasecmp(line, "Cookie:", 7) == 0) {
      strncpy(conn->cookie, line + 7, 127);
    }
  }
  if (0 < content_length) {
    conn->body_len = recv(conn->fd, conn->body, content_length, 0);
  }
  return 0;
}"#,
    ),
    (
        "read_line",
        None,
        r#"char * read_line(conn_t *conn, char *buf, int len)
{
  int i;
  char c;

  for (i = 0; i < len + -1; i = i + 1) {
    if (recv(conn->fd, &c, 1, 0) != 1) {
      return NULL;
    }
    if (c == '\n') break;
    if (c != '\r') {
      buf[i] = c;
    }
  }
  buf[i] = '\0';
  return buf;
}"#,
    ),
    (
        "url_decode",
        None,
        r#"void url_decode(char *s)
{
  char *out;
  uint value;

  out = s;
  while (*s != '\0') {
    if (*s == '%' && isxdigit(s[1]) != 0 && isxdigit(s[2]) != 0) {
      sscanf(s + 1, "%2x", &value);
      *out = (char)value;
      s = s + 3;
    }
    else {
      *out = *s == '+' ? ' ' : *s;
      s = s + 1;
    }
    out = out + 1;
  }
  *out = '\0';
}"#,
    ),
    (
        "send_response",
        None,
        r#"int send_response(conn_t *conn, int status, char *body)
{
  char header[256];
  int n;

  n = snprintf(header, sizeof(header),
               "HTTP/1.1 %d %s\r\nContent-Length: %d\r\nConnection: close\r\n\r\n",
               status, status == 200 ? "OK" : "Error", strlen(body));
  send(conn->fd, header, n, 0);
  return send(conn->fd, body, strlen(body), 0);
}"#,
    ),
    (
        "check_session",
        None,
        r#"bool check_session(conn_t *conn)
{
  char *token;

  token = FUN_00413b10(conn->cookie);
  if (token == NULL) {
    return false;
  }
  return strcmp(token, g_session_token) == 0;
}"#,
    ),
    (
        "FUN_00413b10",
        None,
        r#"char * FUN_00413b10(char *param_1)
{
  char *pcVar1;
  char *pcVar2;

  pcVar1 = strstr(param_1, "session=");
  if (pcVar1 == NULL) {
    return NULL;
  }
  pcVar1 = pcVar1 + 8;
  pcVar2 = strchr(pcVar1, ';');
  if (pcVar2 != NULL) {
    *pcVar2 = '\0';
  }
  return pcVar1;
}"#,
    ),
    (
        "load_config",
        None,
        r#"int load_config(char *path)
{
  FILE *fp;
  char text[2048];
  json_t *root;

  fp = fopen(path, "r");
  if (fp == NULL) {
    return -1;
  }
  text[fread(text, 1, sizeof(text) - 1, fp)] = '\0';
  fclose(fp);
  root = json_parse(text);
  g_config.port = json_get_int(root, "httpd.port", 80);
  return 0;
}"#,
    ),
    (
        "serve_file",
        None,
        r#"int serve_file(conn_t *conn, char *path)
{
  char full[512];
  int fd;

  snprintf(full, sizeof(full), "/www%s", strcmp(path, "/") == 0 ? "/index.html" : path);
  fd = open(full, O_RDONLY);
  if (fd < 0) {
    return send_response(conn, 404, "Not Found");
  }
  conn->content_type = mime_type(full);
  sendfile(conn->fd, fd, NULL, 0x10000);
  close(fd);
  return 0;
}"#,
    ),
    (
        "mime_type",
        None,
        r#"char * mime_type(char *path)
{
  char *ext;

  ext = strrchr(path, '.');
  if (ext == NULL) {
    return "application/octet-stream";
  }
  if (strcmp(ext, ".html") == 0) {
    return "text/html";
  }
  if (strcmp(ext, ".js") == 0) {
    return "text/javascript";
  }
  if (strcmp(ext, ".css") == 0) {
    return "text/css";
  }
  return "application/octet-stream";
}"#,
    ),
    (
        "FUN_00414c88",
        None,
        r#"uint FUN_00414c88(uint param_1, byte *param_2, int param_3)
{
  int iVar1;

  while (param_3 != 0) {
    param_1 = param_1 ^ *param_2;
    for (iVar1 = 0; iVar1 < 8; iVar1 = iVar1 + 1) {
      param_1 = param_1 >> 1 ^ -(param_1 & 1) & 0xedb88320;
    }
    param_2 = param_2 + 1;
    param_3 = param_3 + -1;
  }
  return ~param_1;
}"#,
    ),
    (
        "FUN_00415a20",
        None,
        r#"void FUN_00415a20(void)
{
  int fd;

  fd = open("/var/run/httpd.pid", O_WRONLY | O_CREAT | O_TRUNC, 0644);
  if (-1 < fd) {
    dprintf(fd, "%d\n", getpid());
    close(fd);
  }
}"#,
    ),
];
