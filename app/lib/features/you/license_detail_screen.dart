// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:air/features/you/license_packages.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// The license text of one package, one block per registered entry.
class LicenseDetailScreenView extends HookWidget {
  const LicenseDetailScreenView({super.key, required this.package});

  final LicensePackage package;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    // Resolving paragraphs walks the whole license text, which for the bundled
    // NOTICES takes long enough to drop frames. Off the build so the push
    // animation plays, and memoized so a rebuild does not redo it.
    final paragraphs = useMemoized(
      () => Future(
        () => [for (final entry in package.entries) entry.paragraphs.toList()],
      ),
      [package],
    );

    return Scaffold(
      appBar: AppBar(
        clipBehavior: .none,
        title: Text(package.name),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: FutureBuilder<List<List<LicenseParagraph>>>(
          future: paragraphs,
          builder: (context, snapshot) => switch (snapshot.data) {
            final entries? => _LicenseText(entries: entries),
            null => Center(
              child: SizedBox(
                width: S.s16,
                height: S.s16,
                child: CircularProgressIndicator(
                  strokeWidth: StrokeWidth.px2,
                  valueColor: AlwaysStoppedAnimation<Color>(
                    palette.text.primary,
                  ),
                ),
              ),
            ),
          },
        ),
      ),
    );
  }
}

class _LicenseText extends StatelessWidget {
  const _LicenseText({required this.entries});

  final List<List<LicenseParagraph>> entries;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return ListView.separated(
      padding: const EdgeInsets.all(S.s16),
      itemCount: entries.length,
      separatorBuilder: (_, _) => Divider(
        height: S.s24,
        thickness: StrokeWidth.px0_5,
        color: palette.separator.secondary,
      ),
      itemBuilder: (_, index) => _Entry(paragraphs: entries[index]),
    );
  }
}

class _Entry extends StatelessWidget {
  const _Entry({required this.paragraphs});

  final List<LicenseParagraph> paragraphs;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final bodyStyle = typeScale.body.s.style(color: palette.text.secondary);

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        for (final paragraph in paragraphs)
          if (paragraph.indent == LicenseParagraph.centeredIndent)
            Padding(
              padding: const EdgeInsets.only(bottom: S.s12),
              child: Text(
                paragraph.text,
                textAlign: .center,
                style: typeScale.body.s.style(
                  weight: Weight.emphasized,
                  color: palette.text.primary,
                ),
              ),
            )
          else
            Padding(
              // The registry counts indents in steps, not pixels, so one step
              // maps onto the spacing scale.
              padding: EdgeInsets.only(
                left: paragraph.indent * S.s16,
                bottom: S.s12,
              ),
              child: Text(paragraph.text, style: bodyStyle),
            ),
      ],
    );
  }
}
