// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart' show dbPath;
import 'package:air/core/frb_generated.dart' show RustLib;
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/scroll_behavior.dart';
import 'package:air/ds/material/theme_data.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/l10n/supported_locales.dart';
import 'package:air/platform/logging.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/share/share_payload.dart';
import 'package:air/share/share_screen.dart';
import 'package:flutter/material.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:logging/logging.dart';
import 'package:path/path.dart' as p;

final _log = Logger('ShareMain');

/// Implementation of the share UI entrypoint
///
/// The `@pragma('vm:entry-point')` wrapper the native host runs lives in
/// `main.dart`. Runs in a Flutter engine separate from the main app. Boots
/// the Rust library, fetches the shared payload from the native host and
/// mounts the [ShareScreen]. There is no user cubit, no navigation stack
/// and no push registration.
void shareMain() {
  // The UI is mounted before any asynchronous bootstrap so that a failing
  // or slow initialization is visible instead of an empty sheet.
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const ShareApp());
}

/// The result of the share UI bootstrap
class ShareBootstrap {
  const ShareBootstrap({required this.payload, required this.dbPath});

  final SharePayload payload;
  final String dbPath;
}

Future<ShareBootstrap> _bootstrap() async {
  await initializeDateFormatting();
  await RustLib.init();

  // The extension process survives a dismissed sheet and gets reused for the
  // next share. Its second engine then finds the Rust logger initialized
  // already, and initializing it again fails.
  try {
    final cacheDir = await getCacheDirectory();
    final logWriter = initRustLogging(logFile: p.join(cacheDir, 'app.log'));
    initDartLogging(logWriter);
  } catch (e) {
    _log.info('Skipping logging init: $e');
  }

  final payload = await getSharePayload();
  final path = await dbPath();
  return ShareBootstrap(payload: payload, dbPath: path);
}

/// Minimal app shell around the [ShareScreen]
class ShareApp extends StatefulWidget {
  const ShareApp({super.key});

  @override
  State<ShareApp> createState() => _ShareAppState();
}

class _ShareAppState extends State<ShareApp> {
  late final Future<ShareBootstrap> _bootstrapFuture;

  @override
  void initState() {
    super.initState();
    _bootstrapFuture = _bootstrap();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      scrollBehavior: const AppScrollBehavior(),
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: supportedLocalesWithFallback(
        AppLocalizations.supportedLocales,
        const Locale('en', 'US'),
      ),
      theme: lightTheme,
      darkTheme: darkTheme,
      debugShowCheckedModeBanner: false,
      home: FutureBuilder<ShareBootstrap>(
        future: _bootstrapFuture,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            debugPrint('Share bootstrap failed: ${snapshot.error}');
            _log.severe(
              'Share bootstrap failed',
              snapshot.error,
              snapshot.stackTrace,
            );
            return _BootstrapErrorView(error: snapshot.error!);
          }
          final bootstrap = snapshot.data;
          if (bootstrap == null) {
            return const Scaffold(
              body: Center(child: CircularProgressIndicator()),
            );
          }
          return ShareScreen(
            payload: bootstrap.payload,
            dbPath: bootstrap.dbPath,
          );
        },
      ),
    );
  }
}

class _BootstrapErrorView extends StatelessWidget {
  const _BootstrapErrorView({required this.error});

  final Object error;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(S.s24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(loc.shareScreen_sendFailed, textAlign: TextAlign.center),
                const SizedBox(height: S.s8),
                Text(
                  '$error',
                  textAlign: TextAlign.center,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
                const SizedBox(height: S.s16),
                OutlinedButton(
                  onPressed: () => closeShareHost(success: false),
                  child: Text(loc.errorBanner_ok),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
