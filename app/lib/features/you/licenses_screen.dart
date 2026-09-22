// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/searchfield/searchfield.dart';
import 'package:air/ds/components/searchfield/searchfield_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:air/features/you/license_detail_screen.dart';
import 'package:air/features/you/license_packages.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Lists the packages with registered licenses, both the Dart packages Flutter
/// registers and the Rust crates the app registers at startup.
class LicensesScreenView extends HookWidget {
  const LicensesScreenView({super.key, this.licenses});

  /// Defaults to [LicenseRegistry.licenses].
  final Stream<LicenseEntry>? licenses;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final packagesFuture = useMemoized(
      () => collectLicensePackages(licenses ?? LicenseRegistry.licenses),
    );
    final packages = useFuture(packagesFuture);
    final query = useState('');

    return Scaffold(
      appBar: AppBar(
        clipBehavior: .none,
        title: Text(loc.licensesScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(S.s16),
          child: _Body(
            packages: packages.data,
            query: query.value,
            onQueryChanged: (value) => query.value = value,
          ),
        ),
      ),
    );
  }
}

class _Body extends StatelessWidget {
  const _Body({
    required this.packages,
    required this.query,
    required this.onQueryChanged,
  });

  /// Null while collecting.
  final List<LicensePackage>? packages;
  final String query;
  final ValueChanged<String> onQueryChanged;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final visible = _filter(packages, query);

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        SearchField(
          tokens: SearchFieldTokens.current,
          hintText: loc.licensesScreen_searchHint,
          onChanged: onQueryChanged,
        ),
        const SizedBox(height: S.s12),
        Expanded(
          child: switch (visible) {
            null => const _Loading(),
            [] => Center(
              child: Text(
                loc.licensesScreen_noResults,
                style: typeScale.body.s.style(color: palette.text.tertiary),
              ),
            ),
            _ => _PackageList(packages: visible),
          },
        ),
        if (visible != null) ...[
          const SizedBox(height: S.s8),
          Text(
            loc.licensesScreen_packageCount(visible.length),
            textAlign: .center,
            style: typeScale.body.xs.style(color: palette.text.quaternary),
          ),
        ],
      ],
    );
  }
}

/// Matches on the name alone
List<LicensePackage>? _filter(List<LicensePackage>? packages, String query) {
  final needle = query.trim().toLowerCase();
  if (packages == null || needle.isEmpty) {
    return packages;
  }
  return packages
      .where((package) => package.name.toLowerCase().contains(needle))
      .toList();
}

class _PackageList extends StatelessWidget {
  const _PackageList({required this.packages});

  final List<LicensePackage> packages;

  @override
  Widget build(BuildContext context) => ListView.separated(
    itemCount: packages.length,
    separatorBuilder: (_, _) => const SizedBox(height: S.s8),
    itemBuilder: (_, index) => _PackageRow(package: packages[index]),
  );
}

class _PackageRow extends StatelessWidget {
  const _PackageRow({required this.package});

  final LicensePackage package;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return FieldContainer(
      onTap: () => Navigator.of(context).push(
        MaterialPageRoute(
          builder: (_) => LicenseDetailScreenView(package: package),
        ),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              package.name,
              overflow: .ellipsis,
              style: typeScale.body.regular.style(),
            ),
          ),
          AppIcon.chevronRight(size: S.s16, color: palette.text.tertiary),
        ],
      ),
    );
  }
}

class _Loading extends StatelessWidget {
  const _Loading();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Center(
      child: SizedBox(
        width: S.s16,
        height: S.s16,
        child: CircularProgressIndicator(
          strokeWidth: StrokeWidth.px2,
          valueColor: AlwaysStoppedAnimation<Color>(palette.text.primary),
        ),
      ),
    );
  }
}
