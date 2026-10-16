# Redrob Recall

[English](./README.md) · **한국어**

Redrob Recall은 Redrob 앱들의 로컬 메모리입니다. 내가 고른 폴더를 내 컴퓨터에서 임베딩·키워드 색인으로
만들어 최신 상태로 유지하고, Redrob Office·Chat·Code·Canvas 같은 앱이 하나의 루프백 API로 그 색인을
검색합니다. 폴더를 추가하고, 검색하고, 내 파일을 근거로 답하는 질문을 던지는 얇은 창도 함께 있습니다.

파일, 추출한 텍스트, 임베딩, 색인 전체는 기기 안에 남습니다. 검색은 무료이고 로컬입니다. **Ask**만
질문과 일치한 몇 개의 문단을 Redrob 콘솔로 보내며, 이때 Redrob Launcher로 이 컴퓨터에 로그인한 계정을
씁니다. 파일 이름, 경로, 파일 전체, 색인은 절대 보내지 않습니다.

## 구성

| 부분 | 위치 | 역할 |
| --- | --- | --- |
| `redrob-recall-core` | `crates/redrob-recall-core` | 라이브러리로서의 메모리 레이어: 추출(PDF, DOCX, TXT, Markdown, RST, CSV, JSON, HTML, XML, 로그), 분절, SQLite + FTS5 키워드 색인, Qdrant Edge에 담긴 로컬 다국어 임베딩, 하이브리드 검색, 폴더 감시, 라이브러리 저장소. 창도 네트워크도 없습니다. |
| `redrob-recall-api` | `crates/redrob-recall-api` | 다른 Redrob 앱이 호출하는 루프백 HTTP API(127.0.0.1, 베어러 토큰 하나)와 그 타입드 Rust 클라이언트. [docs/API.md](docs/API.md). |
| `redrob-recall` | `apps/redrob-recall` | 데스크톱 창(eframe/egui, wgpu)과 CLI: `serve`, `index <dir>`, `search <query>`. 창이 열려 있는 동안 API도 서비스합니다. |

모든 데이터는 `~/.redrob/recall`에 있습니다(`REDROB_RECALL_DATA_DIR`로 바꿀 수 있음):

- `metadata.db`: 문서 메타데이터, 추출한 문단, FTS5 색인
- `qdrant-edge/`: 로컬 벡터 색인
- `models/`: 내려받은 임베딩 모델(`multilingual-e5-small`, 384차원)
- `local-api-token`, `local-api-address`: 같은 기기의 클라이언트가 API를 찾고 인증하는 방법
- `backups/`: `metadata.db`의 일관된 복사본([백업과 복구](docs/RECOVERY.md))

원본 파일은 이 디렉터리에 복사되지 않습니다.

## 창

Redrob Canvas와 같은 구조입니다. 상단 바에 보기 탭(Ctrl+1..4), 라이브러리·도움말 메뉴, 계정 칩;
왼쪽에 라이브러리 트리(폴더마다 문서 수와 색인 상태, 고르면 검색 범위가 그 폴더로 좁혀짐);
오른쪽에 메시지 패널(Ctrl+Shift+M: 상태 기록과 읽을 수 없었던 파일 목록); 아래 상태 줄에 색인
진행률과 일시 중지 / 다시 시작. 밝게 / 어둡게는 시스템을 따르고(도움말 > 테마), 언어는 Redrob
Launcher의 언어를, 없으면 시스템을 따릅니다(도움말 > 언어). 도움말 > 단축키에 모든 키가 있습니다.

- **라이브러리**: 색인 상태와 개수, 진행 중인 작업; 폴더 카드마다 표시·제거; 처음 실행하면 폴더
  추가 버튼 하나와, 폴더를 창에 끌어다 놓는 드롭 영역.
- **검색**: 입력하는 대로 검색합니다(시작할 때와 Ctrl+K에 포커스). 의미와 키워드를 함께 따져 순위를
  매긴 결과: 제목, 쪽, 한 줄로 줄인 경로(전체 경로는 마우스를 올리면), 일치한 문단, 점수. ↑ / ↓ 선택,
  Enter 열기, Ctrl+Enter 파일 관리자에서 표시, Esc 지우기. 따옴표 구문, `-제외어`, `heading:단어`,
  `접두어*`가 동작합니다.
- **질문**: 질문과 `[1]`, `[2]` 인용이 붙은 콘솔의 답. 인용을 누르면 출처로 이동하고 파일을 엽니다.
  공유 로그인이 필요하며, 없으면 로컬에서 찾은 출처만 보여 줍니다.
- **설정**: 한 페이지 -- 계정, 답변 모델, 색인(파일 형식, 문단 크기), 라이브러리 위치, 로컬 API,
  언어와 테마 -- 적용 / 되돌리기; 아래에 상태 점검, 백업, 색인 지우기.

## 로그인

Recall에는 자체 로그인이 없습니다. [Redrob Launcher](https://github.com/redrob-labs/redrob-launcher/releases/latest)에서
한 번 로그인하면 Recall을 포함한 이 컴퓨터의 모든 Redrob 앱이 그 계정을 씁니다. 런처가 공유
`account.json`을 쓰고 Recall이 읽습니다(바뀌면 바로 알아챕니다). `REDROB_API_KEY` 환경변수나 Recall
데이터 폴더의 자체 `account.json`이 있으면 그것이 공유 파일보다 우선합니다. Recall은 키를 쓰거나
기록하거나 화면에 보여 주지 않습니다.

## 다른 앱에서 쓰기

```bash
TOKEN="$(cat ~/.redrob/recall/local-api-token)"
BASE="$(cat ~/.redrob/recall/local-api-address)"   # 기본 http://127.0.0.1:47331

curl -H "Authorization: Bearer $TOKEN" "$BASE/health"

curl -X POST "$BASE/v1/search" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"인도 출시 결정","limit":8}'

curl -X POST "$BASE/v1/ask-context" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"인도 건은 어떻게 결정했지?","maxSources":6}'
```

Rust에서는 `redrob-recall-api`를 `client` 기능으로 의존하고 `RecallClient::discover()`를 호출합니다.
모든 경로는 [docs/API.md](docs/API.md)에 있습니다.

## 개발 실행

Rust 1.95.0(`rust-toolchain.toml`이 고름). Linux에서는 egui·wgpu·rfd가 링크하는 X11/Wayland/GL
헤더가 필요합니다:

```bash
sudo apt-get install -y libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
  libgl1-mesa-dev libgtk-3-dev
```

```bash
cargo run -p redrob-recall                       # 창
cargo run -p redrob-recall -- serve              # API만
cargo run -p redrob-recall -- index ~/Documents  # 터미널에서 폴더 색인
cargo run -p redrob-recall -- search "예산"
```

첫 색인이나 검색 때 임베딩 모델을 `~/.redrob/recall/models`에 내려받고, 그 뒤로는 오프라인으로
동작합니다. CI가 돌리는 게이트:

```bash
cargo fmt --all -- --check
cargo clippy -p redrob-recall -p redrob-recall-core -p redrob-recall-api --all-targets --profile ci -- -D warnings
cargo test -p redrob-recall-core -p redrob-recall-api --profile ci
python3 scripts/check-brand.py && python3 scripts/check-version.py
```

### Qdrant Edge 컴파일러 호환

워크스페이스는 Qdrant Edge 0.8.0을 고정하는데, 이 버전은 Rust 1.95에서 아직 기능 게이트 뒤에 있는
표준 라이브러리 매크로를 씁니다. `.cargo/config.toml`이 그 크레이트 속성 하나만 좁게 켭니다
(`RUSTC_BOOTSTRAP`과 `-Zcrate-attr=feature(assert_matches)`). 안정판에서 빌드되는 상류 릴리스와
함께 제거하고, 범위를 넓히지 마세요.

## 릴리스

Linux x86_64(`.tar.gz`, `.deb`), Apple 실리콘 macOS(`.dmg`, `.zip`; 서명·공증·스테이플), Windows
x64(`-portable.zip`, `-setup.exe`; Azure Artifact Signing 코드서명)와 그 옆의 `SHA256SUMS.txt`,
`redrob-launcher.json`, 그것이 가리키는 `redrob-recall.png` 아이콘을 `.github/workflows/release.yml`이 만듭니다([릴리스 안내](docs/RELEASING.md)).
Windows 설치 프로그램은 영어와 한국어를 지원합니다. Intel Mac은 빌드하지 않습니다: `ort-sys`(`fastembed`
경유)가 `x86_64-apple-darwin`용 ONNX Runtime 바이너리를 내지 않습니다. 자산 이름은 Redrob Launcher가
설치하는 규칙을 따릅니다.

## 프라이버시

- 폴더 선택, 추출, 분절, 임베딩, 두 검색 모두 기기 안에서 일어납니다.
- 검색은 아무것도 어디로도 보내지 않습니다.
- Ask는 질문과 최대 10개의 문단만 보냅니다. 파일 이름, 경로, 파일 전체, 색인은 보내지 않습니다.
  Settings의 **Send the matching passages to Redrob**를 끄면 차단됩니다.
- 로컬 API는 127.0.0.1에서만 듣고 토큰 없는 호출을 모두 거부합니다.
- 분석·텔레메트리 없음.

전체 데이터 흐름과 삭제 절차는 [PRIVACY.md](PRIVACY.md)에 있습니다.

## 라이선스와 출처

Apache License 2.0([LICENSE](LICENSE), [NOTICE](NOTICE)). 내장 구성요소는 각자의 라이선스를
유지합니다([THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)). 질의 문법은 bloop의 것을 따르고 벡터
색인은 `qdrant-edge` 크레이트입니다([UPSTREAM_NOTICES.md](UPSTREAM_NOTICES.md),
[docs/upstream-sources.toml](docs/upstream-sources.toml)). Qdrant와 Qdrant Edge는 Qdrant Solutions
GmbH의 프로젝트이며, Redrob Recall은 Redrob의 제품이고 공식 Qdrant 배포본이 아닙니다.

## 문서

- [로컬 API](docs/API.md)
- [아키텍처](docs/ARCHITECTURE.md)
- [릴리스 절차](docs/RELEASING.md)
- [백업과 복구](docs/RECOVERY.md)
- [FTS5를 유지한 이유](docs/index-engine-decision.md)와 [질의 문법](docs/upstream-grammar.md)
- [프라이버시](PRIVACY.md), [보안](SECURITY.md), [변경 기록](CHANGELOG.md)
