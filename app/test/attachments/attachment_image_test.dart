// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/attachments/attachments.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';

const physicalSize = Size(1000, 2800);

final file = UiAttachment(
  attachmentId: 42.attachmentId(),
  filename: 'example.png',
  contentType: 'image/png',
  size: 10 * 1024 * 1024,
  imageMetadata: const UiImageMetadata(
    blurhash: "LEHLk~WB2yk8pyo0adR*.7kCMdnj",
    width: 100,
    height: 50,
  ),
);

Map<AttachmentId, UiAttachmentStatus> testStatuses = {
  1.attachmentId(): const UiAttachmentStatus.pending(),
  2.attachmentId(): UiAttachmentStatus.progress(BigInt.from(7 * 1024 * 1024)),
  3.attachmentId(): const UiAttachmentStatus.failed(),
  4.attachmentId(): const UiAttachmentStatus.notFound(),
  5.attachmentId(): const UiAttachmentStatus.completed(),
};

class _ImageTestBubble extends StatelessWidget {
  const _ImageTestBubble({required this.attachmentId, required this.isSender});

  final AttachmentId attachmentId;
  final bool isSender;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(Spacing.px20),
      child: Container(
        constraints: const BoxConstraints(maxHeight: 300, maxWidth: 300),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(Spacing.px20),
        ),
        child: AttachmentImage(
          attachment: file.copyWith(attachmentId: attachmentId),
          imageMetadata: file.imageMetadata!,
          isSender: isSender,
          fit: BoxFit.cover,
        ),
      ),
    );
  }
}

void main() {
  setUpAll(() {
    registerFallbackValue(0.attachmentId());
  });

  late MockAttachmentsRepository attachmentsRepository;

  setUp(() async {
    attachmentsRepository = MockAttachmentsRepository();
    when(
      () => attachmentsRepository.statusStream(
        attachmentId: any(named: 'attachmentId'),
      ),
    ).thenAnswer(
      (invocation) => Stream.value(
        testStatuses[invocation.namedArguments[#attachmentId] as AttachmentId]!,
      ),
    );
  });

  Widget buildSubject({required bool isSender}) => Builder(
    builder: (context) {
      return RepositoryProvider<AttachmentsRepository>.value(
        value: attachmentsRepository,
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: Scaffold(
            body: Padding(
              padding: const EdgeInsets.all(Spacing.px16),
              child: SizedBox(
                width: double.infinity,
                child: Column(
                  spacing: Spacing.px16,
                  crossAxisAlignment: .center,
                  children: [
                    for (final testStatus in testStatuses.entries) ...[
                      Text(switch (testStatus.value) {
                        UiAttachmentStatus_Pending() => "Pending",
                        UiAttachmentStatus_Progress() => "Progress",
                        UiAttachmentStatus_Completed() => "Completed",
                        UiAttachmentStatus_Failed() => "Failed",
                        UiAttachmentStatus_NotFound() => "Not Found",
                      }),
                      _ImageTestBubble(
                        attachmentId: testStatus.key,
                        isSender: isSender,
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
        ),
      );
    },
  );

  group('AttachmentImage Upload', () {
    testWidgets('renders correctly', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject(isSender: true));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_upload_image.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject(isSender: true));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_image_upload_dark_mode.png'),
      );
    });
  });

  group('AttachmentImage Download', () {
    testWidgets('renders correctly', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject(isSender: false));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_image_download.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject(isSender: false));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_image_download_dark_mode.png'),
      );
    });
  });
}
