# 기여 가이드

[English](./CONTRIBUTING.md) · **한국어**

기여해 주셔서 감사합니다. 이 문서는 저장소의 작업 합의입니다. 브랜치를 어떻게 이름 짓고, 머지 전에
무엇이 초록이어야 하며, 어떤 변경도 조용히 깨뜨려서는 안 되는 제품의 약속이 무엇인지 정합니다.

## 제품의 약속

레드롭 리콜은 **로컬 우선**입니다. 파일, 추출한 텍스트, 임베딩, 검색 인덱스 전체가 기기에
남습니다. 검색은 로컬에서 돌고 무료입니다. 사용자가 질문하면 그 질문과 관련 있는 발췌 몇 개만
기기를 떠납니다. 파일명, 경로, 파일 전체, 인덱스는 떠나지 않습니다.

이 문장은 README에도 있고 제품 안에도 있으니 계약으로 취급합니다.

- **네트워크 호출을 추가하는 것은 세부사항이 아니라 리뷰 항목입니다.** 무엇이 기기를 떠나고 왜
  떠나는지 pull request에 적습니다.
- **경로나 파일명은 절대 API로 보내지 않습니다.** 발췌 텍스트가 경계입니다.
- **로컬 산출물은 앱 자신의 데이터 디렉터리 안에 둡니다**: `metadata.db`, `qdrant-edge/`,
  `models/`, 그리고 생성되는 `local-api-token`. 이 토큰은 기기를 떠나지 않더라도 자격증명이므로
  Unix에서 `0600`입니다.

## 브랜치 모델

장수 브랜치는 둘입니다. `develop`은 작업이 들어오는 곳이고, `main`은 릴리스된 상태입니다.

- **`develop`**이 기본 브랜치이자 통합 브랜치입니다. 작업 브랜치는 여기서 떼고, pull request는
  다시 여기로 엽니다. 저장소를 클론하면 `develop`에 있습니다.
- **`main`**은 릴리스된 상태입니다. `release/*`와 `hotfix/*` 브랜치에서 온 pull request만 받고,
  릴리스 태그는 여기서 뗍니다. 그 밖의 것은 여기 머지되지 않습니다.
- **작업 브랜치**는 `develop`에서 뗀 `<type>/<short-slug>` 형식입니다. 예: `fix/index-restart`,
  `feat/pdf-extract`. 타입은 `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `perf`.
- **머지는 squash만** 하고, 머지 시 브랜치를 삭제합니다. pull request 하나가 커밋 하나가 되므로
  `git log develop`이 그래프가 아니라 변경 목록으로 읽힙니다. squash 커밋의 본문은 작업 중에 쓴
  메시지를 이어붙인 것이 아니라 pull request 본문입니다.

두 브랜치는 이 문서가 아니라 GitHub ruleset이 강제합니다.

- 직접 push 금지: 모든 변경은 pull request로 들어옵니다.
- force push 금지, 브랜치 삭제 금지.
- linear history.
- 필수 상태 검사 통과. 최신 tip 기준으로 통과할 필요는 없으므로, 봇 업데이트가 줄을 서도 하나씩
  rebase하고 다시 돌릴 필요는 없습니다.
- 머지 전에 리뷰 스레드가 해결되어야 합니다.

ruleset은 pull request가 *어느* 브랜치에서 왔는지는 표현할 수 없으므로, "`release/*`와 `hotfix/*`만
`main`에 머지된다"는 이 문서가 담고 리뷰어가 지키는 관례입니다. 그중 한 부분은 기계가 검사합니다.
릴리스 워크플로가 `origin/main`에서 도달할 수 없는 커밋의 태그는 빌드를 거부하므로, `develop`에서
바로 태그를 떼면 출시되는 대신 실패합니다.

### 릴리스

```bash
git switch develop && git pull
git switch -c release/v0.2.0
# bump the version, update the changelog, run the release check
# open a pull request into main and merge it, then tag main:
git switch main && git pull
git tag -a v0.2.0 -m "Redrob Recall v0.2.0"
git push origin v0.2.0
# bring main's release commit back so develop does not fall behind:
git switch -c chore/sync-main-to-develop main
# open a pull request into develop
```

hotfix도 같은 모양인데, `hotfix/*`를 `develop`이 아니라 `main`에서 떼고 양쪽에 머지합니다.

저장소를 포크하고, 브랜치를 자기 포크에 push한 다음, 거기서 pull request를 엽니다. 기여에 쓰기
권한은 필요하지 않으며, 포크에서 온 pull request는 저장소 시크릿 없이 CI를 돌립니다.

## 평소 작업

```bash
git switch develop && git pull
git switch -c fix/short-description

npm install
npm run check
npm run build

cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace

# open a pull request into develop
```

추출 관련 변경에는 그것을 실제로 돌려보는 파일이 함께 와야 합니다. 이 제품이 읽는 형식(PDF, DOCX,
HWP, CSV 등)은 실제 문서만이 드러내는 방식으로 깨집니다. 텍스트 레이어가 없는 PDF, 숫자가 날짜로
잡히는 스프레드시트처럼요.

### 커밋

제목은 명령형으로 씁니다. 본문에는 _왜_를 적고, 기기를 떠나는 것에 영향이 있는 변경이라면 diff에만
남기지 말고 본문에 밝힙니다.

## CI가 검사하는 것

`ci.yml`의 두 잡 모두 머지 전에 필수이고, 어느 쪽도 시크릿을 쓰지 않으므로 포크에서 온 pull
request도 이 저장소의 브랜치와 같은 검사를 받습니다.

| 검사 | 실행 내용 |
| --- | --- |
| **Verify source and desktop core** | `npm ci` 다음 `npm run verify` — 전체 빌드, 포매팅, 테스트, Clippy |
| **Rust security advisories** | lockfile로 고정된 의존성 집합에 대한 `cargo audit` |

advisory 스캔은 매일 바뀌는 데이터베이스를 읽으므로, 의존성을 건드리지 않은 브랜치에서도 빨갛게 될
수 있습니다. 내 탓이라고 단정하기 전에 `develop`의 같은 잡을 확인하세요. 실제 발견이라면 advisory를
허용하지 말고 crate를 올립니다.

**`release.yml`**은 태그가 붙으면 빌드하고 배포합니다. 구조상 메인테이너 전용입니다. `push: tags`로
발동하고, 태그 커밋이 `main`의 조상인지 검증하며, 자격증명을 쓰는 잡은 `production-release` 환경
뒤에 둡니다. 이 조상 검사가 브랜치 모델을 실제로 만들어 줍니다. `develop`에서 뗀 태그는 출시되는
대신 거부됩니다.

push 전에 `npm run check`와 `cargo test --workspace`를 돌리세요.
