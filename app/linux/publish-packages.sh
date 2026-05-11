#!/usr/bin/env bash
# publish-pkg-repo.sh — Upload a .deb or .rpm to a signed S3 package repository.
#
# Usage:
#   publish-pkg-repo.sh [OPTIONS] <package-file>
#
# Options:
#   -t, --type      deb|rpm           (auto-detected from file extension if omitted)
#   -b, --s3-bucket S3_BUCKET         (or env S3_BUCKET)
#   -p, --prefix    S3_PREFIX         (optional key prefix, e.g. "releases")
#       --track     TRACK             release track / APT suite name, e.g.
#                                     "testing", "stable"
#                                     (or env TRACK, default: testing)
#   -a, --arch      ARCH              deb: e.g. "amd64"; rpm: e.g. "x86_64"
#                                     (auto-detected from package if omitted)
#   -k, --gpg-key   GPG_KEY_ID        GPG key fingerprint/email to sign with
#                                     (or env GPG_KEY_ID)
#       --no-sign                     Skip GPG signing entirely
#       --s3-endpoint URL             S3 endpoint URL (or env AWS_S3_ENDPOINT)
#       --repository-base-url URL     Public download base URL shown in client-
#                                     setup instructions (or env REPOSITORY_BASE_URL)
#       --dry-run                     Print aws commands without executing them
#   -h, --help                        Show this help
#
# Dependencies:
#   DEB: apt-ftparchive, dpkg-deb, gpg, aws
#   RPM: createrepo_c (or createrepo), rpm, gpg, aws
#
# Environment variables (all overridable via flags):
#   S3_BUCKET, S3_PREFIX, GPG_KEY_ID, GPG_PASSPHRASE, AWS_S3_ENDPOINT,
#   REPOSITORY_BASE_URL, TRACK
#
#   AWS_S3_ENDPOINT overrides the S3 endpoint URL (e.g. for MinIO, Cloudflare R2,
#   Backblaze B2). When unset, the default AWS S3 endpoint is used.
#
# Examples:
#   # Upload a .deb
#   S3_BUCKET=my-repo GPG_KEY_ID=releases@example.com \
#     ./publish-pkg-repo.sh myapp_1.0_amd64.deb
#
#   # Upload an .rpm
#   S3_BUCKET=my-repo GPG_KEY_ID=releases@example.com \
#     ./publish-pkg-repo.sh myapp-1.0.x86_64.rpm
#
#   # Dry-run without signing
#   ./publish-pkg-repo.sh --dry-run --no-sign -b my-repo myapp_1.0_amd64.deb

set -euo pipefail

# Colours
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[info]${NC}  $*"; }
ok()    { echo -e "${GREEN}[ok]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[warn]${NC}  $*"; }
die()   { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }

# Defaults
PKG_FILE=""

PKG_TYPE=""
BUCKET="${S3_BUCKET:-}"
PREFIX="${S3_PREFIX:-}"
# Release track / APT suite name. Appears in the DEB Release file
# (Suite/Codename) and the dists/<track>/ path that clients reference.
TRACK="${TRACK:-testing}"
# APT requires a component in the path/Release file. Hardcoded to "main" since
# we don't need to expose the distinction — single-component repos are standard
# for small projects (Tailscale, Brave, GitHub CLI, ...).
COMPONENT="main"
ARCH=""
GPG_KEY="${GPG_KEY_ID:-}"
SIGN=true
DRY_RUN=false
S3_ENDPOINT="${AWS_S3_ENDPOINT:-}"
REPOSITORY_BASE_URL="${REPOSITORY_BASE_URL:-}"
# Persistent build directory under the invocation cwd. Kept on disk so repeat
# publishes can reuse downloaded packages and repodata across runs.
WORKDIR="$(pwd)/package-builds"

# Argument parsing
usage() {
  sed -n '/^# Usage:/,/^[^#]/{ /^[^#]/d; s/^# \{0,2\}//; p }' "$0"
  exit 0
}

while [[ $# -gt 0 ]]; do
  case $1 in
    -t|--type)      PKG_TYPE="$2";    shift 2 ;;
    -b|--s3-bucket) BUCKET="$2";      shift 2 ;;
    -p|--prefix)    PREFIX="$2";      shift 2 ;;
    --track)        TRACK="$2";       shift 2 ;;
    -a|--arch)      ARCH="$2";        shift 2 ;;
    -k|--gpg-key)   GPG_KEY="$2";     shift 2 ;;
    --no-sign)      SIGN=false;       shift   ;;
    --s3-endpoint)  S3_ENDPOINT="$2"; shift 2 ;;
    --repository-base-url)     REPOSITORY_BASE_URL="$2";    shift 2 ;;
    --dry-run)      DRY_RUN=true;     shift   ;;
    -h|--help)      usage ;;
    -*)             die "Unknown option: $1" ;;
    *)              PKG_FILE="$1";    shift   ;;
  esac
done

# Validation
[[ -n "$PKG_FILE" ]]  || die "No package file specified. Run with -h for usage."
[[ -f "$PKG_FILE" ]]  || die "File not found: $PKG_FILE"
[[ -n "$BUCKET" ]]    || die "S3 bucket not set (--s3-bucket / S3_BUCKET)."
$SIGN && [[ -z "$GPG_KEY" ]] && die "GPG key not set (--gpg-key / GPG_KEY_ID). Use --no-sign to skip."

# Auto-detect package type
if [[ -z "$PKG_TYPE" ]]; then
  case "$PKG_FILE" in
    *.deb) PKG_TYPE="deb" ;;
    *.rpm) PKG_TYPE="rpm" ;;
    *)     die "Cannot detect package type from filename. Use --type deb|rpm." ;;
  esac
fi
[[ "$PKG_TYPE" == "deb" || "$PKG_TYPE" == "rpm" ]] || die "Invalid type: $PKG_TYPE"

# Trim any trailing slash so client-setup snippets don't end up with "//".
# Fall back to a placeholder when no base URL was provided, so the snippet is
# still copy-pasteable as a template.
REPOSITORY_BASE_URL="${REPOSITORY_BASE_URL%/}"
REPOSITORY_BASE_URL="${REPOSITORY_BASE_URL:-https://<cdn-or-bucket-url>}"

# Many S3-compatible providers (Upcloud, some MinIO versions, etc.) reject the
# newer flow checksums that AWS CLI v2 sends by default, producing errors like
# "The Content-SHA256 you specified did not match what we received". Falling
# back to "when_required" only sends checksums when the server asks for them.
if [[ -n "$S3_ENDPOINT" ]]; then
  export AWS_REQUEST_CHECKSUM_CALCULATION="${AWS_REQUEST_CHECKSUM_CALCULATION:-when_required}"
  export AWS_RESPONSE_CHECKSUM_VALIDATION="${AWS_RESPONSE_CHECKSUM_VALIDATION:-when_required}"
fi

# Helpers
require() {
  for cmd in "$@"; do
    command -v "$cmd" &>/dev/null || die "Required command not found: $cmd"
  done
}

aws_cmd() {
  local endpoint_args=()
  if [[ -n "$S3_ENDPOINT" ]]; then
    endpoint_args=(--endpoint-url "$S3_ENDPOINT")
  fi
  if $DRY_RUN; then
    echo -e "${YELLOW}[dry-run]${NC} aws ${endpoint_args[*]} $*"
  else
    aws "${endpoint_args[@]}" "$@"
  fi
}

gpg_passphrase_args() {
  # Pipe the passphrase via a file descriptor so it never appears in ps output.
  if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
    echo "--batch --yes --passphrase-fd 3"
  else
    echo "--batch --yes"
  fi
}

run_gpg() {
  # Wrapper that injects the passphrase FD when available.
  if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
    gpg $(gpg_passphrase_args) "$@" 3<<<"${GPG_PASSPHRASE}"
  else
    gpg --batch --yes "$@"
  fi
}

s3_path() {
  # Build s3://bucket/[prefix/]<suffix>
  local suffix="$1"
  if [[ -n "$PREFIX" ]]; then
    echo "s3://${BUCKET}/${PREFIX%/}/${suffix}"
  else
    echo "s3://${BUCKET}/${suffix}"
  fi
}

# DEB publishing
publish_deb() {
  require apt-ftparchive dpkg-deb aws

  # Auto-detect arch from the .deb if not provided
  if [[ -z "$ARCH" ]]; then
    ARCH="$(dpkg-deb -f "$PKG_FILE" Architecture)"
    info "Detected architecture: $ARCH"
  fi

  local pool_dir="${WORKDIR}/pool/${COMPONENT}"
  local dists_dir="${WORKDIR}/dists/${TRACK}/${COMPONENT}/binary-${ARCH}"
  local key_dir="${WORKDIR}/keys"
  mkdir -p "$pool_dir" "$dists_dir" "$key_dir"

  local s3_deb
  s3_deb="$(s3_path "deb")"

  # Pull existing repo state
  # Hydrate pool/ and dists/${TRACK}/ from S3 so the regenerated Packages and
  # Release files describe everything that was there before, plus the new
  # package. Without this, every publish would silently orphan old .debs by
  # producing an index that lists only the current upload.
  info "Syncing existing pool from ${s3_deb}/pool..."
  aws_cmd s3 sync "${s3_deb}/pool" "${WORKDIR}/pool"
  info "Syncing existing dists from ${s3_deb}/dists/${TRACK}..."
  aws_cmd s3 sync "${s3_deb}/dists/${TRACK}" "${WORKDIR}/dists/${TRACK}"

  # Copy package into pool
  info "Staging package into pool..."
  cp "$PKG_FILE" "${pool_dir}/"

  # Sign the .deb with dpkg-sig (optional)
  if $SIGN; then
    require gpg
    info "Signing .deb with GPG key: $GPG_KEY"
    if command -v dpkg-sig &>/dev/null; then
      run_gpg --export --armor "$GPG_KEY" > "${key_dir}/gpg-key.asc"
      dpkg-sig --sign builder -k "$GPG_KEY" "${pool_dir}/$(basename "$PKG_FILE")"
    else
      warn "dpkg-sig not found — skipping per-package signature (Release will still be signed)."
      run_gpg --export --armor "$GPG_KEY" > "${key_dir}/gpg-key.asc"
    fi
  fi

  # Generate Packages index
  info "Running apt-ftparchive packages..."
  (
    cd "$WORKDIR"
    apt-ftparchive packages "pool/${COMPONENT}" > "${dists_dir}/Packages"
    gzip  -9 -k "${dists_dir}/Packages"
    bzip2 -9 -k "${dists_dir}/Packages"
    xz    -9 -k "${dists_dir}/Packages"
  )

  # Generate Release
  info "Running apt-ftparchive release..."
  local release_dir="${WORKDIR}/dists/${TRACK}"
  apt-ftparchive \
    -o "APT::FTPArchive::Release::Origin=Custom"     \
    -o "APT::FTPArchive::Release::Label=Custom"      \
    -o "APT::FTPArchive::Release::Suite=${TRACK}"     \
    -o "APT::FTPArchive::Release::Codename=${TRACK}"  \
    -o "APT::FTPArchive::Release::Components=${COMPONENT}" \
    -o "APT::FTPArchive::Release::Architectures=${ARCH}" \
    release "$release_dir" > "${release_dir}/Release"

  # Sign Release
  if $SIGN; then
    info "Signing Release file..."
    run_gpg --default-key "$GPG_KEY" \
      --armor --detach-sign \
      --output "${release_dir}/Release.gpg" \
      "${release_dir}/Release"

    run_gpg --default-key "$GPG_KEY" \
      --armor --clearsign \
      --output "${release_dir}/InRelease" \
      "${release_dir}/Release"
  fi

  # Upload to S3
  info "Uploading pool (immutable, long TTL)..."
  aws_cmd s3 sync "${WORKDIR}/pool" "${s3_deb}/pool" \
    --cache-control "public, max-age=31536000, immutable" \
    --acl public-read

  info "Uploading dists (index files, short TTL)..."
  aws_cmd s3 sync "${WORKDIR}/dists" "${s3_deb}/dists" \
    --cache-control "public, max-age=300" \
    --acl public-read

  if $SIGN && [[ -f "${key_dir}/gpg-key.asc" ]]; then
    info "Uploading public GPG key..."
    aws_cmd s3 sync "${key_dir}" "${s3_deb}" \
      --cache-control "public, max-age=86400" \
      --acl public-read
  fi

  ok "DEB repository published to ${s3_deb}"
  echo
  echo "Client setup:"
  echo "  curl -fsSL ${REPOSITORY_BASE_URL}/deb/gpg-key.asc \\"
  echo "    | sudo gpg --dearmor -o /usr/share/keyrings/${BUCKET}-keyring.gpg"
  echo "  echo \"deb [signed-by=/usr/share/keyrings/${BUCKET}-keyring.gpg] \\"
  echo "    ${REPOSITORY_BASE_URL}/deb ${TRACK} ${COMPONENT}\" \\"
  echo "    | sudo tee /etc/apt/sources.list.d/${BUCKET}.list"
  echo "  sudo apt update"
}

# RPM publishing
publish_rpm() {
  # Prefer createrepo_c, fall back to createrepo
  local createrepo_bin
  if command -v createrepo_c &>/dev/null; then
    createrepo_bin="createrepo_c"
  elif command -v createrepo &>/dev/null; then
    createrepo_bin="createrepo"
    warn "createrepo_c not found, falling back to createrepo (slower, no zchunk)."
  else
    die "Neither createrepo_c nor createrepo found. Install createrepo_c."
  fi

  require rpm rpmsign aws

  # Auto-detect arch from the .rpm if not provided
  if [[ -z "$ARCH" ]]; then
    ARCH="$(rpm -qp --queryformat '%{ARCH}' "$PKG_FILE" 2>/dev/null)"
    info "Detected architecture: $ARCH"
  fi

  local repo_dir="${WORKDIR}/${COMPONENT}/${ARCH}"
  local key_dir="${WORKDIR}/keys"
  mkdir -p "$repo_dir" "$key_dir"

  local s3_rpm
  s3_rpm="$(s3_path "rpm")"

  # Pull existing repo state
  # Hydrate the component/arch directory (existing .rpms + repodata/) so
  # createrepo_c --update can incrementally extend the previous metadata
  # instead of producing a repo that only references the new package.
  info "Syncing existing repo from ${s3_rpm}/${COMPONENT}/${ARCH}..."
  aws_cmd s3 sync "${s3_rpm}/${COMPONENT}/${ARCH}" "${repo_dir}"

  # Copy package
  info "Staging package..."
  cp "$PKG_FILE" "${repo_dir}/"

  # Sign the .rpm
  if $SIGN; then
    require gpg
    info "Signing .rpm with GPG key: $GPG_KEY"

    # Export key into RPM keyring (rpmmacros approach)
    local macros_file="${HOME}/.rpmmacros"
    local macros_bak=""
    if [[ -f "$macros_file" ]]; then
      macros_bak="$(mktemp)"
      cp "$macros_file" "$macros_bak"
    fi

    cat > "$macros_file" <<EOF
%_signature gpg
%_gpg_name  ${GPG_KEY}
%_gpg_path  ${GNUPGHOME:-${HOME}/.gnupg}
%__gpg      $(command -v gpg)
EOF

    if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
      echo "${GPG_PASSPHRASE}" | \
        rpm --addsign "${repo_dir}/$(basename "$PKG_FILE")" \
          --define "_gpg_sign_cmd_extra_args --passphrase-fd 0 --pinentry-mode loopback"
    else
      rpm --addsign "${repo_dir}/$(basename "$PKG_FILE")"
    fi

    # Restore original .rpmmacros
    if [[ -n "$macros_bak" ]]; then
      mv "$macros_bak" "$macros_file"
    else
      rm -f "$macros_file"
    fi

    # Export public key for clients
    run_gpg --export --armor "$GPG_KEY" > "${key_dir}/gpg-key.asc"
  fi

  # Generate repodata
  info "Running ${createrepo_bin}..."
  "$createrepo_bin" --update "$repo_dir"

  # Sign repomd.xml
  if $SIGN; then
    info "Signing repomd.xml..."
    run_gpg --default-key "$GPG_KEY" \
      --armor --detach-sign \
      --output "${repo_dir}/repodata/repomd.xml.asc" \
      "${repo_dir}/repodata/repomd.xml"
  fi

  # Upload to S3
  info "Uploading .rpm packages (immutable, long TTL)..."
  aws_cmd s3 sync "${repo_dir}" "${s3_rpm}/${COMPONENT}/${ARCH}" \
    --exclude "repodata/*" \
    --cache-control "public, max-age=31536000, immutable" \
    --acl public-read

  info "Uploading repodata (short TTL)..."
  aws_cmd s3 sync "${repo_dir}/repodata" \
    "${s3_rpm}/${COMPONENT}/${ARCH}/repodata" \
    --cache-control "public, max-age=300" \
    --acl public-read

  if $SIGN && [[ -f "${key_dir}/gpg-key.asc" ]]; then
    info "Uploading public GPG key..."
    aws_cmd s3 sync "${key_dir}" "${s3_rpm}" \
      --cache-control "public, max-age=86400" \
      --acl public-read
  fi

  ok "RPM repository published to ${s3_rpm}"
  echo
  echo "Client setup:"
  echo "  sudo rpm --import ${REPOSITORY_BASE_URL}/rpm/gpg-key.asc"
  echo "  cat <<'EOF' | sudo tee /etc/yum.repos.d/${BUCKET}.repo"
  echo "  [${BUCKET}]"
  echo "  name=${BUCKET} repository"
  echo "  baseurl=${REPOSITORY_BASE_URL}/rpm/${COMPONENT}/${ARCH}"
  echo "  enabled=1"
  echo "  gpgcheck=1"
  echo "  repo_gpgcheck=1"
  echo "  gpgkey=${REPOSITORY_BASE_URL}/rpm/gpg-key.asc"
  echo "  EOF"
}

# Entry point
info "Package : $(basename "$PKG_FILE")"
info "Type    : $PKG_TYPE"
info "Bucket  : s3://${BUCKET}"
info "Track   : $TRACK"
info "Workdir : $WORKDIR"
$SIGN && info "GPG key : $GPG_KEY" || info "Signing : DISABLED"
$DRY_RUN && warn "Dry-run mode — no changes will be made."
echo

case "$PKG_TYPE" in
  deb) publish_deb ;;
  rpm) publish_rpm ;;
esac