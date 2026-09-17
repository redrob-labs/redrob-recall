# Redrob Recall

[English](./README.md) · **한국어**

Redrob Recall은 폴더를 검색하고 내 파일에 근거해 질문에 답하는 로컬 우선 데스크톱 앱입니다. 파일, 추출한 텍스트, 임베딩, 검색 색인 전체가 기기에 남습니다. 검색은 무료이고 로컬에서 돌아갑니다. **Ask**를 쓸 때만 질문과 관련 있는 텍스트 발췌 몇 개가 기존 Redrob API로 전송되며, 파일명·경로·파일 전체·색인 전체는 보내지 않습니다.

**컴퓨터 안의 모든 것을 검색 가능하게.**

## 무엇을 하는가

- Docker나 별도 데이터베이스 서버 없이 선택한 폴더를 색인합니다.
- PDF, DOCX, TXT, Markdown, RST, CSV, JSON, HTML, XML, 로그 파일을 읽습니다.
- 다국어 의미 검색과 키워드 매칭을 함께 씁니다.
- 선택한 폴더를 감시해 파일이 바뀌면 로컬 라이브러리를 갱신합니다.
- 모든 결과를 원래 위치에서 열거나 파일 탐색기에 표시합니다.
- 로컬 파일로 되돌아가는 번호가 붙은 인용과 함께 Redrob 답변을 만듭니다.
- 루프백 전용, 베어러 인증이 걸린 API를 선택적으로 제공합니다.

## 구조

| 계층 | 구현 |
| --- | --- |
| 데스크톱 셸 | Tauri 2 |
| 인터페이스 | React 19 + TypeScript + Vite |
| 메타데이터·키워드 검색 | SQLite + FTS5 |
| 벡터 검색 | Qdrant Edge, 프로세스 내 임베드 |
| 로컬 임베딩 | FastEmbed `MultilingualE5Small`, 384차원 |
| 파일 감시 | `notify` |
| Redrob 답변 | 설정된 Redrob base URL의 `POST /chat/completions` |

애플리케이션 데이터는 `~/.redrob/recall`에 저장됩니다.

- `metadata.db`: 문서 메타데이터, 추출한 구절, FTS 데이터
- `qdrant-edge/`: 로컬 의미 색인
- `models/`: 내려받은 로컬 임베딩 모델
- `local-api-token`: 생성된 로컬 API 베어러 토큰(Unix에서 `0600`)

원본 파일은 이 디렉터리로 복사되지 않습니다. 일관성 있는 메타데이터 백업은 `~/.redrob/recall/backups`에 보관합니다. [백업과 복구](docs/RECOVERY.md)를 보세요.

## 신뢰성과 복구

Redrob Recall은 시작할 때 메타데이터 데이터베이스를 검증하고, 순서가 정해진 스키마 마이그레이션을 적용하며, 마이그레이션 전 백업을 만듭니다. 문서는 의미 벡터가 커밋될 때까지 pending 상태로 남으므로, 중단된 색인은 다음 스캔에서 복구할 수 있습니다. 손상된 파생 색인은 조용히 덮어쓰지 않고 격리하며, 연결이 끊긴 감시 드라이브를 삭제된 라이브러리로 취급하지 않습니다.

**설정 → 저장소**에서 상태 점검을 돌리거나 메타데이터 백업을 직접 만들 수 있습니다. 백업에는 추출한 구절과 경로가 들어 있으니 운영체제의 전체 디스크 암호화로 보호해야 합니다.

## 개발 환경에서 실행

### 사전 요구 사항

- Node.js 22 이상
- npm 11 이상
- Rust 1.95.0 (`rust-toolchain.toml`이 자동으로 선택)
- 운영체제별 Tauri 2 시스템 의존성

Linux 패키지 이름은 배포판마다 다릅니다. 일반적인 Debian/Ubuntu 환경에는 WebKitGTK 4.1, GTK 3, librsvg, 빌드 도구가 필요합니다.

```bash
sudo apt update
sudo apt install -y build-essential curl wget file libwebkit2gtk-4.1-dev \
  libappindicator3-dev librsvg2-dev libssl-dev patchelf
```

의존성을 설치하고 데스크톱 앱을 실행합니다.

```bash
npm install
npm run tauri dev
```

화면 개발용 프런트엔드 전용 데모 데이터도 있습니다.

```bash
npm run dev
# http://localhost:1420/?demo=1 열기
```

처음 색인하거나 검색할 때 다국어 임베딩 모델을 내려받습니다. 그 다음부터는 색인과 검색이 네트워크 없이 로컬에서 동작합니다.

## 인스톨러 빌드

서명 없는 로컬 개발 번들:

```bash
npm ci
npm run tauri build
```

Tauri는 플랫폼별 번들을 `src-tauri/target/release/bundle/`에 씁니다. 프로덕션 인스톨러는 리뷰를 거친 **Signed desktop release** 워크플로만 만듭니다. 그 워크플로는 [릴리스 안내](docs/RELEASING.md)에 적힌 외부 업데이터·Apple·Windows 서명 시크릿을 요구하며, 자격 증명이 없으면 서명되지 않은 산출물을 발행하는 대신 릴리스를 중단합니다.

발행된 빌드는 **설정 → 업데이트**에서 서명된 stable 업데이트 채널을 확인합니다. 업데이트 메타데이터와 인스톨러 서명은 설치 전에 Tauri가 검증합니다. 로컬 개발 빌드에는 의도적으로 업데이트 키와 엔드포인트를 설정하지 않습니다.

### Qdrant Edge 컴파일러 호환성

애플리케이션은 Qdrant Edge 0.8.0을 고정합니다. 이 릴리스는 Rust 1.95에서 아직 feature 게이트가 걸린 표준 라이브러리 매크로를 씁니다. `.cargo/config.toml`이 `RUSTC_BOOTSTRAP`과 `-Zcrate-attr=feature(assert_matches)`로 필요한 크레이트 속성만 좁게 켭니다. 안정 버전 Rust에서 그대로 빌드되는 상류 릴리스로 올릴 때 이 우회를 제거하세요. 켜는 feature 범위를 넓히지 마세요.

## Redrob 연결

1. `https://console.redrob.ai/api-keys`에서 워크스페이스 API 키를 만들거나 고릅니다.
2. **Ask** 또는 **설정 → Redrob 연결**을 엽니다.
3. 키를 붙여 넣습니다. 앱은 운영체제 자격 증명 관리자에 저장합니다.

로컬 색인과 검색에는 Redrob이 필요하지 않습니다. Ask는 제공한 키에 연결된 워크스페이스와 잔액을 사용합니다. 이 앱은 Redrob Console을 변경하지 않습니다.

## 프라이버시 동작

- 폴더 선택, 텍스트 추출, 청킹, 임베딩, 키워드 검색, 의미 검색은 모두 기기에서 일어납니다.
- 검색은 질의나 결과를 Redrob에 전송하지 않습니다.
- Ask는 질문과 관련 발췌 문자열 최대 10개를 보냅니다. 파일명, 경로, 원본 파일, 색인 전체는 보내지 않습니다.
- **Ask가 관련 발췌를 전송하도록 허용**을 끄면 백엔드 경계에서 Ask가 차단됩니다.
- 분석·텔레메트리 SDK는 포함되어 있지 않습니다.

전체 데이터 흐름과 삭제 방법은 [PRIVACY.md](PRIVACY.md)를 보세요.

## 로컬 API

API는 `127.0.0.1`에만 바인딩하고 기본 포트는 `47331`입니다. 모든 엔드포인트가 `~/.redrob/recall/local-api-token`의 토큰을 요구합니다.

```bash
TOKEN="$(cat ~/.redrob/recall/local-api-token)"

curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:47331/health

curl -X POST http://127.0.0.1:47331/v1/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"India launch decision","limit":8,"filters":{"extensions":[],"pathPrefix":null}}'

curl -X POST http://127.0.0.1:47331/v1/ask \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"What did we decide about India?","sourceIds":[],"maxSources":6}'
```

로컬 API 스위치나 포트를 바꾸면 Redrob Recall을 재시작한 뒤에 적용됩니다. 로컬 API를 통한 Ask도 UI와 같은 프라이버시 설정과 Redrob 연결 요구 사항을 따릅니다.

## 자주 쓰는 명령

```bash
npm run check                    # TypeScript와 잠긴 Rust 검사
npm run check:version            # 릴리스 버전이 모두 일치하는지 확인
npm run test:rust                # 마이그레이션·검증·API 오류 테스트
npm run verify                   # 전체 빌드, 감사, 포매팅, 테스트, Clippy
npm run format                   # Prettier와 rustfmt
npm audit                        # JavaScript 의존성 감사
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## 기여

[CONTRIBUTING.md](CONTRIBUTING.md)에 브랜치 모델과 필수 검사가 있습니다. `main`은 룰셋으로 보호되며, 병합은 squash 전용입니다.

## 라이선스와 귀속

Redrob Recall은 [Apache License 2.0](LICENSE)으로 배포됩니다. [NOTICE](NOTICE)를 보세요. 임베드된 오픈소스 구성요소는 각자의 라이선스를 유지합니다. [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)를 보세요. Qdrant와 Qdrant Edge는 Qdrant Solutions GmbH의 프로젝트이며, Redrob Recall은 Redrob 제품이고 공식 Qdrant 배포판으로 제시하지 않습니다.

## 프로젝트 문서

- [아키텍처와 신뢰 경계](docs/ARCHITECTURE.md)
- [서명된 릴리스 절차](docs/RELEASING.md)
- [백업과 복구](docs/RECOVERY.md)
- [릴리스 QA 체크리스트](docs/QA_CHECKLIST.md)
- [프라이버시와 삭제](PRIVACY.md)
- [보안 정책](SECURITY.md)
- [체인지로그](CHANGELOG.md)
- [서드파티 고지](THIRD_PARTY_NOTICES.md)

빌드를 발행하기 전에 전체 로컬 검증을 돌리세요.

```bash
npm run verify
```

버전 일관성 검사, 프로덕션 프런트엔드 빌드, JavaScript 의존성 감사, Rust 포매팅, Rust 테스트, 경고를 오류로 취급하는 Clippy를 실행합니다. Linux 호스트에서는 Tauri 네이티브 개발 패키지를 먼저 설치해야 합니다. CI는 잠긴 의존성 그래프에 대해 RustSec 광고 데이터베이스를 별도로 돌립니다.
