// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:collection';
import 'dart:convert';
import 'dart:io';

import 'package:args/args.dart';
import 'package:path/path.dart' as p;

const _defaultArb = 'lib/l10n/app_en.arb';
const _defaultSearchRoots = ['lib', 'test'];
const _defaultExtensions = ['.dart', '.kt', '.swift', '.java', '.m', '.mm'];
const _defaultExcludeDirs = ['lib/l10n'];
const _defaultIncludeFiles = ['lib/l10n/app_localizations_extension.dart'];

Future<void> main(List<String> arguments) async {
  final parser = _buildParser();
  late final ArgResults argResults;
  try {
    argResults = parser.parse(arguments);
  } on ArgParserException catch (error) {
    stderr.writeln('Error: ${error.message}');
    stderr.writeln(parser.usage);
    exitCode = 64;
    return;
  }

  if (argResults['help'] as bool) {
    stdout.writeln(parser.usage);
    return;
  }

  final projectRoot = Directory(argResults['project-root'] as String).absolute.path;
  String resolve(String input) =>
      p.normalize(p.isAbsolute(input) ? input : p.join(projectRoot, input));

  final arbPath = resolve(argResults['arb'] as String);
  final arbFile = File(arbPath);
  if (!arbFile.existsSync()) {
    stderr.writeln('ARB file not found: $arbPath');
    exitCode = 1;
    return;
  }

  final searchRoots = (argResults['search-root'] as List<String>).map(resolve).toList();
  final includeExts =
      (argResults['ext'] as List<String>).map((ext) => ext.startsWith('.') ? ext : '.$ext').toSet();
  final excludeDirs = (argResults['exclude-dir'] as List<String>).map(resolve).toList();
  final includeFiles = (argResults['include-file'] as List<String>).map(resolve).toSet();
  final keepMetadata = argResults['keep-metadata'] as bool;
  final verbose = argResults['verbose'] as bool;
  final apply = argResults['apply'] as bool;
  final safeMode = argResults['safe'] as bool;
  final allowDirty = argResults['allow-dirty'] as bool;

  if (safeMode && !apply) {
    stderr.writeln('--safe requires --apply so changes can be written.');
    exitCode = 64;
    return;
  }

  if (safeMode && !allowDirty) {
    _ensureCleanGitWorkspace(projectRoot);
  }

  final keys = _loadKeys(arbFile);
  if (keys.isEmpty) {
    stdout.writeln('No keys found in ${arbFile.path}.');
    return;
  }

  final candidateFiles = _collectCandidateFiles(
    searchRoots: searchRoots,
    includeExtensions: includeExts,
    excludeDirs: excludeDirs,
    includeFiles: includeFiles,
  );

  if (candidateFiles.isEmpty) {
    stderr.writeln('No files matched the provided search criteria.');
    exitCode = 1;
    return;
  }

  final unusedKeys = _findUnusedKeys(
    keys: keys,
    files: candidateFiles,
    verbose: verbose,
    projectRoot: projectRoot,
  );

  if (unusedKeys.isEmpty) {
    stdout.writeln('✅ All localization keys are referenced.');
    if (safeMode) {
      stdout.writeln('Safe mode: skipping flutter commands because nothing was pruned.');
    }
    return;
  }

  stdout.writeln('Found ${unusedKeys.length} unused key(s):');
  for (final key in unusedKeys.toList()..sort()) {
    stdout.writeln(' • $key');
  }

  if (!apply) {
    stdout.writeln('\nDry-run mode; pass --apply to remove them.');
    return;
  }

  final mirrorArbs = (argResults['mirror-arb'] as List<String>).map(resolve).toList();
  final autoMirrors = _discoverSiblingArbs(arbPath);
  final targetSet = LinkedHashSet<String>()
    ..add(arbPath)
    ..addAll(mirrorArbs)
    ..addAll(autoMirrors);
  final allTargets = targetSet.toList();
  final totalFiles = allTargets.length;

  var totalRemoved = 0;
  for (final target in allTargets) {
    final file = File(target);
    if (!file.existsSync()) {
      stderr.writeln('Skipping missing ARB: $target');
      continue;
    }
    totalRemoved += _pruneArbFile(file, unusedKeys, keepMetadata);
  }

  stdout.writeln('\nRemoved $totalRemoved entries across $totalFiles file(s).');

  if (safeMode) {
    _runFlutterCommand(['gen-l10n'], workingDirectory: projectRoot);
    final analyzeTargets = _buildAnalyzeTargets(searchRoots, projectRoot);
    final analyzeArgs = analyzeTargets.isEmpty ? ['analyze'] : ['analyze', ...analyzeTargets];
    _runFlutterCommand(analyzeArgs, workingDirectory: projectRoot);
  }
}

ArgParser _buildParser() {
  return ArgParser()
    ..addFlag(
      'help',
      abbr: 'h',
      negatable: false,
      help: 'Show this usage information.',
    )
    ..addFlag(
      'apply',
      negatable: false,
      help: 'Rewrite ARB file(s). Without this flag the script only reports unused keys.',
    )
    ..addFlag(
      'verbose',
      negatable: false,
      help: 'Print a line whenever keys are found in a file.',
    )
    ..addFlag(
      'keep-metadata',
      negatable: false,
      help: 'Keep @metadata entries even if their base key is removed.',
    )
    ..addOption(
      'project-root',
      defaultsTo: '.',
      help: 'Resolve relative paths against this directory (defaults to current working directory).',
    )
    ..addOption(
      'arb',
      defaultsTo: _defaultArb,
      help: 'Canonical ARB file to inspect.',
    )
    ..addMultiOption(
      'mirror-arb',
      valueHelp: 'path',
      help: 'Additional ARB files that should be pruned alongside the canonical file.',
    )
    ..addMultiOption(
      'search-root',
      valueHelp: 'path',
      defaultsTo: _defaultSearchRoots,
      help: 'Directories to scan for localization usages.',
    )
    ..addMultiOption(
      'ext',
      valueHelp: '.dart',
      defaultsTo: _defaultExtensions,
      help: 'File extensions to include while scanning.',
    )
    ..addMultiOption(
      'exclude-dir',
      valueHelp: 'path',
      defaultsTo: _defaultExcludeDirs,
      help: 'Directories to skip when searching for usages.',
    )
    ..addMultiOption(
      'include-file',
      valueHelp: 'path',
      defaultsTo: _defaultIncludeFiles,
      help: 'Files that are always scanned even if they live in excluded directories.',
    )
    ..addFlag(
      'safe',
      negatable: false,
      help:
          'Require a clean git workspace (unless --allow-dirty), prune with --apply, then run flutter gen-l10n and flutter analyze on the scanned directories.',
    )
    ..addFlag(
      'allow-dirty',
      negatable: false,
      help: 'Skip the git clean check (useful when running with --safe).',
    );
}

List<String> _loadKeys(File arbFile) {
  final raw = arbFile.readAsStringSync();
  final data = jsonDecode(raw) as Map<String, dynamic>;
  return data.keys.where((key) => !key.startsWith('@')).toList();
}

List<File> _collectCandidateFiles({
  required List<String> searchRoots,
  required Set<String> includeExtensions,
  required List<String> excludeDirs,
  required Set<String> includeFiles,
}) {
  final files = <File>[];
  for (final rootPath in searchRoots) {
    final directory = Directory(rootPath);
    if (!directory.existsSync()) {
      continue;
    }
    for (final entity in directory.listSync(recursive: true, followLinks: false)) {
      if (entity is! File) {
        continue;
      }
      final path = p.normalize(entity.path);
      if (includeFiles.contains(path)) {
        files.add(entity);
        continue;
      }
      if (!includeExtensions.contains(p.extension(path))) {
        continue;
      }
      if (_isExcluded(path, excludeDirs)) {
        continue;
      }
      files.add(entity);
    }
  }
  return files;
}

bool _isExcluded(String filePath, List<String> excludeDirs) {
  for (final dir in excludeDirs) {
    if (p.equals(filePath, dir) || p.isWithin(dir, filePath)) {
      return true;
    }
  }
  return false;
}

Set<String> _findUnusedKeys({
  required List<String> keys,
  required List<File> files,
  required bool verbose,
  required String projectRoot,
}) {
  final unused = keys.toSet();
  for (final file in files) {
    late final String text;
    try {
      text = file.readAsStringSync();
    } on IOException catch (error) {
      stderr.writeln('⚠️  Skipping ${file.path}: $error');
      continue;
    }
    final hits = unused.where((key) => text.contains(key)).toList();
    if (hits.isNotEmpty) {
      unused.removeAll(hits);
      if (verbose) {
        final relPath = _relative(file.path, projectRoot);
        stderr.writeln('✔ Found ${hits.length} key(s) in $relPath');
      }
      if (unused.isEmpty) {
        break;
      }
    }
  }
  return unused;
}

void _ensureCleanGitWorkspace(String workingDirectory) {
  final result = Process.runSync(
    'git',
    ['status', '--porcelain'],
    workingDirectory: workingDirectory,
  );
  if (result.exitCode != 0) {
    stderr
      ..writeln('Failed to run git status in $workingDirectory')
      ..write(result.stderr);
    exit(1);
  }

  final output = (result.stdout as String).trim();
  if (output.isNotEmpty) {
    stderr.writeln(
      'Safe mode: working tree is dirty. Commit or stash your changes before pruning localizations.',
    );
    exit(1);
  }
}

int _pruneArbFile(File file, Set<String> unusedKeys, bool keepMetadata) {
  final contents = file.readAsStringSync();
  final data = Map<String, dynamic>.from(jsonDecode(contents) as Map<String, dynamic>);

  final baseCount = unusedKeys.where(data.containsKey).length;
  final metadataCount = keepMetadata
      ? 0
      : unusedKeys.map((key) => '@$key').where(data.containsKey).length;
  final removed = baseCount + metadataCount;

  if (removed == 0) {
    return 0;
  }

  final preserved = _removeKeysPreservingWhitespace(
    contents,
    unusedKeys,
    keepMetadata: keepMetadata,
  );
  file.writeAsStringSync(preserved);
  return removed;
}

String _relative(String target, String root) {
  final normalizedTarget = p.normalize(target);
  final normalizedRoot = p.normalize(root);
  if (p.isWithin(normalizedRoot, normalizedTarget)) {
    return p.relative(normalizedTarget, from: normalizedRoot);
  }
  return normalizedTarget;
}

void _runFlutterCommand(List<String> args, {required String workingDirectory}) {
  final label = args.join(' ');
  stdout.writeln('Safe mode: running flutter $label …');
  final result = Process.runSync(
    'flutter',
    args,
    workingDirectory: workingDirectory,
  );
  stdout.write(result.stdout);
  stderr.write(result.stderr);
  if (result.exitCode != 0) {
    stderr.writeln('flutter $label failed with exit code ${result.exitCode}.');
    exit(result.exitCode);
  }
}

List<String> _discoverSiblingArbs(String arbPath) {
  final directoryPath = p.dirname(arbPath);
  final directory = Directory(directoryPath);
  if (!directory.existsSync()) {
    return const [];
  }

  final files = <String>[];
  for (final entity in directory.listSync()) {
    if (entity is! File) {
      continue;
    }
    final normalized = p.normalize(entity.path);
    if (normalized == p.normalize(arbPath)) {
      continue;
    }
    if (p.extension(normalized) == '.arb') {
      files.add(normalized);
    }
  }
  return files;
}

List<String> _buildAnalyzeTargets(List<String> searchRoots, String projectRoot) {
  final targets = LinkedHashSet<String>();
  for (final path in searchRoots) {
    if (Directory(path).existsSync()) {
      targets.add(_relative(path, projectRoot));
    }
  }

  if (targets.isEmpty) {
    for (final fallback in ['lib', 'test']) {
      final candidate = p.join(projectRoot, fallback);
      if (Directory(candidate).existsSync()) {
        targets.add(fallback);
      }
    }
  }

  return targets.toList();
}

String _removeKeysPreservingWhitespace(
  String content,
  Set<String> unusedKeys, {
  required bool keepMetadata,
}) {
  final lines = content.split('\n');
  final kept = <String>[];
  final keyPattern = RegExp(r'^"([^"]+)":');

  var index = 0;
  while (index < lines.length) {
    final line = lines[index];
    final trimmedLeft = line.trimLeft();

    if (!trimmedLeft.startsWith('"')) {
      kept.add(line);
      index++;
      continue;
    }

    final match = keyPattern.firstMatch(trimmedLeft);
    if (match == null) {
      kept.add(line);
      index++;
      continue;
    }

    final keyName = match.group(1)!;
    final isMetadata = keyName.startsWith('@');
    final baseKey = isMetadata ? keyName.substring(1) : keyName;
    final shouldRemove = unusedKeys.contains(baseKey) && (!isMetadata || !keepMetadata);

    if (!shouldRemove) {
      kept.add(line);
      index++;
      continue;
    }

    var braceDepth = _initialBraceDepth(trimmedLeft);
    while (braceDepth > 0 && index + 1 < lines.length) {
      index++;
      braceDepth += _lineBraceDelta(lines[index]);
    }

    index++;
  }

  var output = kept.join('\n');
  output = output.replaceAll(RegExp(r',(\s*})'), r'$1');
  if (!output.endsWith('\n')) {
    output = '$output\n';
  }
  return output;
}

int _initialBraceDepth(String line) {
  if (!_startsObjectValue(line)) {
    return 0;
  }
  return _lineBraceDelta(line);
}

bool _startsObjectValue(String line) {
  final colonIndex = line.indexOf(':');
  if (colonIndex == -1) {
    return false;
  }
  for (var i = colonIndex + 1; i < line.length; i++) {
    final char = line[i];
    if (char.trim().isEmpty) {
      continue;
    }
    return char == '{';
  }
  return false;
}

int _lineBraceDelta(String line) {
  var delta = 0;
  var inString = false;
  var isEscaped = false;
  for (var i = 0; i < line.length; i++) {
    final char = line[i];
    if (isEscaped) {
      isEscaped = false;
      continue;
    }
    if (char == '\\') {
      isEscaped = true;
      continue;
    }
    if (char == '"') {
      inString = !inString;
      continue;
    }
    if (inString) {
      continue;
    }
    if (char == '{') {
      delta++;
    } else if (char == '}') {
      delta--;
    }
  }
  return delta;
}
