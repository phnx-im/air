// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';
import 'dart:io';

Future<void> main() async {
  final repoRoot = Directory.current.parent;

  final currentVersion = await _determineCurrentVersion(repoRoot);
  final nextVersion = _incrementMinor(currentVersion);
  stdout.writeln('Bumping version $currentVersion -> $nextVersion');

  await _ensureCargoSetVersionAvailable(repoRoot);
  await _ensureGitCliffAvailable(repoRoot);

  await _runProcess('cargo', [
    'set-version',
    '--workspace',
    nextVersion,
  ], workingDirectory: repoRoot);

  await _updateFlutterVersion(repoRoot, nextVersion);
  stdout.writeln('Updated Flutter version to $nextVersion+1');

  final changelogSection = await _runProcess(
    'git-cliff',
    ['--unreleased', '--tag', 'v$nextVersion'],
    workingDirectory: repoRoot,
    captureStdout: true,
  );

  await _prependChangelog(repoRoot, changelogSection.trimRight(), nextVersion);
  stdout.writeln('Prepended changelog section for v$nextVersion');

  await _createTag(repoRoot, nextVersion);
  stdout.writeln('Created git tag v$nextVersion');
}

Future<String> _determineCurrentVersion(Directory repoRoot) async {
  final metadataJson = await _runProcess(
    'cargo',
    ['metadata', '--format-version', '1', '--no-deps'],
    workingDirectory: repoRoot,
    captureStdout: true,
  );

  final metadata = jsonDecode(metadataJson) as Map<String, dynamic>;
  final members = (metadata['workspace_members'] as List).cast<String>();
  if (members.isEmpty) {
    throw StateError('No workspace members found in cargo metadata output');
  }

  final packages =
      (metadata['packages'] as List)
          .cast<Map<String, dynamic>>()
          .map((pkg) => pkg.cast<String, dynamic>())
          .toList();
  final firstId = members.first;
  final package = packages.firstWhere(
    (pkg) => pkg['id'] == firstId,
    orElse: () => throw StateError('Could not find metadata for $firstId'),
  );

  final version = package['version'] as String?;
  if (version == null) {
    throw StateError('Package $firstId does not have a version');
  }
  return version;
}

String _incrementMinor(String version) {
  final parts = version.split('.');
  if (parts.length != 3) {
    throw StateError('Version "$version" is not in MAJOR.MINOR.PATCH format');
  }
  final major = int.tryParse(parts[0]);
  final minor = int.tryParse(parts[1]);
  if (major == null || minor == null) {
    throw StateError('Unable to parse "$version" as integers');
  }
  return '$major.${minor + 1}.0';
}

Future<void> _updateFlutterVersion(
  Directory repoRoot,
  String newVersion,
) async {
  final pubspec = File('${repoRoot.path}/app/pubspec.yaml');
  if (!pubspec.existsSync()) {
    throw StateError('pubspec.yaml not found at ${pubspec.path}');
  }

  final content = pubspec.readAsStringSync();
  final versionLine = RegExp(r'^version:\s*.+$', multiLine: true);
  if (!versionLine.hasMatch(content)) {
    throw StateError('Could not locate version line in pubspec.yaml');
  }
  final updated = content.replaceFirst(versionLine, 'version: $newVersion+1');
  pubspec.writeAsStringSync(updated);
}

Future<void> _prependChangelog(
  Directory repoRoot,
  String newSection,
  String newVersion,
) async {
  if (newSection.isEmpty) {
    throw StateError('git-cliff produced empty output for v$newVersion');
  }

  final changelog = File('${repoRoot.path}/CHANGELOG.md');
  if (!changelog.existsSync()) {
    throw StateError('CHANGELOG.md not found at ${changelog.path}');
  }

  final previous = changelog.readAsStringSync();
  final buffer =
      StringBuffer()
        ..writeln(newSection)
        ..writeln()
        ..write(previous);
  changelog.writeAsStringSync(buffer.toString());
}

Future<void> _ensureCargoSetVersionAvailable(Directory repoRoot) async {
  final result = await Process.run('cargo', [
    'set-version',
    '--help',
  ], workingDirectory: repoRoot.path);
  if (result.exitCode != 0) {
    throw StateError(
      'cargo set-version is unavailable. Install cargo-edit via '
      '`cargo install cargo-edit` and re-run the bump.',
    );
  }
}

Future<void> _ensureGitCliffAvailable(Directory repoRoot) async {
  final result = await Process.run('git-cliff', [
    '--version',
  ], workingDirectory: repoRoot.path);
  if (result.exitCode != 0) {
    throw StateError(
      'git-cliff is unavailable. Install it (e.g. `cargo install git-cliff`) '
      'and re-run the bump.',
    );
  }
}

Future<void> _createTag(Directory repoRoot, String version) async {
  final tagName = 'v$version';
  final existing = await _runProcess(
    'git',
    ['tag', '--list', tagName],
    workingDirectory: repoRoot,
    captureStdout: true,
  );
  if (existing.trim().isNotEmpty) {
    throw StateError('Git tag $tagName already exists.');
  }
  await _runProcess('git', ['tag', tagName], workingDirectory: repoRoot);
}

Future<String> _runProcess(
  String command,
  List<String> args, {
  required Directory workingDirectory,
  bool captureStdout = false,
}) async {
  final result = await Process.run(
    command,
    args,
    workingDirectory: workingDirectory.path,
  );
  if (result.exitCode != 0) {
    stderr.writeln(result.stdout);
    stderr.writeln(result.stderr);
    throw ProcessException(
      command,
      args,
      'Command exited with ${result.exitCode}',
      result.exitCode,
    );
  }
  if (captureStdout) {
    return (result.stdout as String).trimRight();
  }
  return '';
}
