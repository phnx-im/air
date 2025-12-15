// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';
import 'dart:io';
import 'package:path/path.dart' as p;
import 'package:vector_graphics_compiler/vector_graphics_compiler.dart' as vgc;

/// Directory containing raw SVGs (relative to app/).
const _svgDir = 'lib/ui/icons/svg';

/// Output generated Dart file (relative to app/).
const _outputFile = 'lib/ui/icons/generated_svg_icons.dart';

Future<void> main() async {
  if (!vgc.initializeTessellatorFromFlutterCache()) {
    stderr.writeln('Failed to initialize tessellator; run `flutter precache`.');
    exit(1);
  }
  if (!vgc.initializePathOpsFromFlutterCache()) {
    stderr.writeln('Failed to initialize path_ops; run `flutter precache`.');
    exit(1);
  }

  final svgDirectory = Directory(_svgDir);
  if (!svgDirectory.existsSync()) {
    stderr.writeln('SVG directory not found: $_svgDir');
    exit(1);
  }

  final svgFiles =
      svgDirectory
          .listSync()
          .whereType<File>()
          .where((f) => f.path.toLowerCase().endsWith('.svg'))
          .toList()
        ..sort((a, b) => a.path.compareTo(b.path));

  if (svgFiles.isEmpty) {
    stderr.writeln('No SVG files found in $_svgDir');
    exit(1);
  }

  final enumEntries = <String>[];
  final loaderEntries = StringBuffer();

  for (final file in svgFiles) {
    final name = p.basenameWithoutExtension(file.path);
    final enumName = _toEnumName(name);
    enumEntries.add(enumName);

    final svgString = await file.readAsString();
    final bytes = vgc.encodeSvg(xml: svgString, debugName: name);
    final base64Data = base64Encode(bytes);

    loaderEntries.writeln(
      "  CompiledSvgIcon.$enumName: const _InlineBytesLoader('$base64Data'),",
    );
  }

  final buffer = StringBuffer();
  buffer.writeln('// GENERATED CODE - DO NOT MODIFY BY HAND');
  buffer.writeln("// Generated via app/tool/compile_svg_icons.dart");
  buffer.writeln();
  buffer.writeln("import 'dart:convert';");
  buffer.writeln("import 'package:flutter/services.dart';");
  buffer.writeln("import 'package:flutter/widgets.dart';");
  buffer.writeln("import 'package:vector_graphics/vector_graphics.dart';");
  buffer.writeln();
  buffer.writeln('enum CompiledSvgIcon { ${enumEntries.join(', ')} }');
  buffer.writeln();
  buffer.writeln(
    'final Map<CompiledSvgIcon, _InlineBytesLoader> _compiledSvgLoaders = {',
  );
  buffer.write(loaderEntries.toString());
  buffer.writeln('};');
  buffer.writeln();
  buffer.writeln(
    'BytesLoader compiledSvgLoader(CompiledSvgIcon icon) => _compiledSvgLoaders[icon]!;',
  );
  buffer.writeln();
  buffer.writeln('Widget compiledSvgWidget(');
  buffer.writeln('  CompiledSvgIcon icon, {');
  buffer.writeln('  double? size,');
  buffer.writeln('  Color? color,');
  buffer.writeln('  BoxFit fit = BoxFit.contain,');
  buffer.writeln('  Alignment alignment = Alignment.center,');
  buffer.writeln('}) {');
  buffer.writeln('  return VectorGraphic(');
  buffer.writeln('    loader: compiledSvgLoader(icon),');
  buffer.writeln('    width: size,');
  buffer.writeln('    height: size,');
  buffer.writeln('    fit: fit,');
  buffer.writeln('    alignment: alignment,');
  buffer.writeln('    colorFilter: color != null');
  buffer.writeln('        ? ColorFilter.mode(color, BlendMode.srcIn)');
  buffer.writeln('        : null,');
  buffer.writeln('  );');
  buffer.writeln('}');
  buffer.writeln();
  buffer.writeln('class _InlineBytesLoader extends BytesLoader {');
  buffer.writeln('  const _InlineBytesLoader(this.base64Data);');
  buffer.writeln('  final String base64Data;');
  buffer.writeln('  @override');
  buffer.writeln(
    '  Future<ByteData> loadBytes(BuildContext? context) async => ByteData.sublistView(base64Decode(base64Data));',
  );
  buffer.writeln('  @override');
  buffer.writeln('  Object cacheKey(BuildContext? context) => this;');
  buffer.writeln('}');

  File(_outputFile).writeAsStringSync(buffer.toString());
  // Keep generated file tidy.
  Process.runSync('dart', ['format', _outputFile], runInShell: true);
  stdout.writeln('Generated $_outputFile with ${svgFiles.length} icons.');
}

String _toEnumName(String name) {
  // Keep it simple: map file-name with dashes/underscores to lowerCamel.
  final segments = name.split(RegExp(r'[-_ ]+')).where((s) => s.isNotEmpty);
  return segments
      .mapIndexed((i, s) => i == 0 ? s.toLowerCase() : _capitalize(s))
      .join();
}

extension<T> on Iterable<T> {
  Iterable<E> mapIndexed<E>(E Function(int i, T e) convert) sync* {
    var index = 0;
    for (final element in this) {
      yield convert(index++, element);
    }
  }
}

String _capitalize(String input) => input.isEmpty
    ? input
    : input[0].toUpperCase() + input.substring(1).toLowerCase();
