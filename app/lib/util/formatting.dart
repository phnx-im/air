import 'dart:math';

extension Formatting on String {
  String spacedInGroupsOf(int chunkSize) {
    final chunks = <String>[];
    for (var i = 0; i < length; i += chunkSize) {
      chunks.add(substring(i, min(i + chunkSize, length)));
    }
    return chunks.join(' ');
  }

  String digitsOnly() {
    return replaceAll(RegExp(r'\D'), "");
  }
}
