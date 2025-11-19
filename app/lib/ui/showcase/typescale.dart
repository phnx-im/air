// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/theme/scale.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';

class Typescale extends StatelessWidget {
  const Typescale({super.key});

  @override
  Widget build(BuildContext context) {
    const sample = 'The quick brown fox jumps over the lazy dog';

    // Collect all sizes from enums with labels, group by identical numeric size.
    final Map<double, List<String>> grouped = {};
    void addAll<T>(
      Iterable<T> values,
      String prefix,
      double Function(T) sizeOf,
      String Function(T) nameOf,
    ) {
      for (final v in values) {
        final sz = sizeOf(v);
        grouped.putIfAbsent(sz, () => []);
        grouped[sz]!.add('$prefix.${nameOf(v)}');
      }
    }

    addAll(HeaderFontSize.values, 'Header', (h) => h.size, (h) => h.name);
    addAll(BodyFontSize.values, 'Body', (b) => b.size, (b) => b.name);
    addAll(LabelFontSize.values, 'Label', (l) => l.size, (l) => l.name);

    final entries = grouped.entries.map((e) {
      e.value.sort();
      return _Entry(e.value, e.key);
    }).toList()..sort((a, b) => b.size.compareTo(a.size));

    Widget row(_Entry e) => Padding(
      padding: const EdgeInsets.symmetric(vertical: 16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 4,
        children: [
          // First row: font size value + enum variants
          Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            spacing: 4,
            children: [
              SizedBox(
                width: 64,
                child: Text(
                  actualUiSize(e.size, context).toStringAsFixed(2),
                  softWrap: false,
                  style: TextStyle(
                    fontSize: LabelFontSize.base.size,
                    color: CustomColorScheme.of(context).text.tertiary,
                  ).withSystemMonospace(),
                ),
              ),
              Expanded(
                child: Text(
                  e.labels.join(', '),
                  softWrap: false,
                  style: TextStyle(
                    fontSize: LabelFontSize.small2.size,
                    color: CustomColorScheme.of(context).text.quaternary,
                    fontWeight: FontWeight.w500,
                  ).withSystemMonospace(),
                ),
              ),
            ],
          ),
          // Second row: sample text
          Row(
            children: [
              Expanded(
                child: Text(
                  sample,
                  softWrap: false,
                  style: TextStyle(
                    fontSize: e.size,
                    fontWeight: FontWeight.w400,
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 24, horizontal: 64),
      child: Center(
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: SizedBox(
            width: 800,
            child: ListView(
              shrinkWrap: true,
              scrollDirection: Axis.vertical,
              children: entries.map(row).toList(),
            ),
          ),
        ),
      ),
    );
  }
}

class _Entry {
  final List<String> labels;
  final double size;
  const _Entry(this.labels, this.size);
}
