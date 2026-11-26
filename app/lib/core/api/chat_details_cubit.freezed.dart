// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'chat_details_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ChatDetailsState {

 UiChatDetails? get chat; List<UiUserId> get members; UiComposerLinkPreview? get linkPreview; List<UiStagedAttachment> get stagedAttachments;
/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ChatDetailsStateCopyWith<ChatDetailsState> get copyWith => _$ChatDetailsStateCopyWithImpl<ChatDetailsState>(this as ChatDetailsState, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as ChatDetailsState;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatDetailsState&&(identical(other.chat, _this.chat) || other.chat == _this.chat)&&const DeepCollectionEquality().equals(other.members, _this.members)&&(identical(other.linkPreview, _this.linkPreview) || other.linkPreview == _this.linkPreview)&&const DeepCollectionEquality().equals(other.stagedAttachments, _this.stagedAttachments));
}


@override
int get hashCode {
  final _this = this as ChatDetailsState;
  return Object.hash(runtimeType,_this.chat,const DeepCollectionEquality().hash(_this.members),_this.linkPreview,const DeepCollectionEquality().hash(_this.stagedAttachments));
}

@override
String toString() {
  final _this = this as ChatDetailsState;
  return 'ChatDetailsState(chat: ${_this.chat}, members: ${_this.members}, linkPreview: ${_this.linkPreview}, stagedAttachments: ${_this.stagedAttachments})';
}


}

/// @nodoc
abstract mixin class $ChatDetailsStateCopyWith<$Res>  {
  factory $ChatDetailsStateCopyWith(ChatDetailsState value, $Res Function(ChatDetailsState) _then) = _$ChatDetailsStateCopyWithImpl;
@useResult
$Res call({
 UiChatDetails? chat, List<UiUserId> members, UiComposerLinkPreview? linkPreview, List<UiStagedAttachment> stagedAttachments
});


$UiComposerLinkPreviewCopyWith<$Res>? get linkPreview;

}
/// @nodoc
class _$ChatDetailsStateCopyWithImpl<$Res>
    implements $ChatDetailsStateCopyWith<$Res> {
  _$ChatDetailsStateCopyWithImpl(this._self, this._then);

  final ChatDetailsState _self;
  final $Res Function(ChatDetailsState) _then;

/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? chat = freezed,Object? members = null,Object? linkPreview = freezed,Object? stagedAttachments = null,}) {
  return _then(ChatDetailsState(
chat: freezed == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails?,members: null == members ? _self.members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,linkPreview: freezed == linkPreview ? _self.linkPreview : linkPreview // ignore: cast_nullable_to_non_nullable
as UiComposerLinkPreview?,stagedAttachments: null == stagedAttachments ? _self.stagedAttachments : stagedAttachments // ignore: cast_nullable_to_non_nullable
as List<UiStagedAttachment>,
  ));
}
/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiComposerLinkPreviewCopyWith<$Res>? get linkPreview {
    if (_self.linkPreview == null) {
    return null;
  }

  return $UiComposerLinkPreviewCopyWith<$Res>(_self.linkPreview!, (value) {
    return _then(_self.copyWith(linkPreview: value));
  });
}
}



/// @nodoc


class _ChatDetailsState extends ChatDetailsState {
  const _ChatDetailsState({this.chat, required  List<UiUserId> members, this.linkPreview,  List<UiStagedAttachment> stagedAttachments = const []}): _members = members,_stagedAttachments = stagedAttachments,super._();
  

@override final  UiChatDetails? chat;
 final  List<UiUserId> _members;
@override List<UiUserId> get members {
  if (_members is EqualUnmodifiableListView) return _members;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_members);
}

@override final  UiComposerLinkPreview? linkPreview;
 final  List<UiStagedAttachment> _stagedAttachments;
@override@JsonKey() List<UiStagedAttachment> get stagedAttachments {
  if (_stagedAttachments is EqualUnmodifiableListView) return _stagedAttachments;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_stagedAttachments);
}


/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$ChatDetailsStateCopyWith<_ChatDetailsState> get copyWith => __$ChatDetailsStateCopyWithImpl<_ChatDetailsState>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _ChatDetailsState&&(identical(other.chat, chat) || other.chat == chat)&&const DeepCollectionEquality().equals(other.members, _members)&&(identical(other.linkPreview, linkPreview) || other.linkPreview == linkPreview)&&const DeepCollectionEquality().equals(other.stagedAttachments, _stagedAttachments));
}


@override
int get hashCode {
    return Object.hash(runtimeType,chat,const DeepCollectionEquality().hash(_members),linkPreview,const DeepCollectionEquality().hash(_stagedAttachments));
}

@override
String toString() {
    return 'ChatDetailsState(chat: $chat, members: $members, linkPreview: $linkPreview, stagedAttachments: $stagedAttachments)';
}


}

/// @nodoc
abstract mixin class _$ChatDetailsStateCopyWith<$Res> implements $ChatDetailsStateCopyWith<$Res> {
  factory _$ChatDetailsStateCopyWith(_ChatDetailsState value, $Res Function(_ChatDetailsState) _then) = __$ChatDetailsStateCopyWithImpl;
@override @useResult
$Res call({
 UiChatDetails? chat, List<UiUserId> members, UiComposerLinkPreview? linkPreview, List<UiStagedAttachment> stagedAttachments
});


@override $UiComposerLinkPreviewCopyWith<$Res>? get linkPreview;

}
/// @nodoc
class __$ChatDetailsStateCopyWithImpl<$Res>
    implements _$ChatDetailsStateCopyWith<$Res> {
  __$ChatDetailsStateCopyWithImpl(this._self, this._then);

  final _ChatDetailsState _self;
  final $Res Function(_ChatDetailsState) _then;

/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? chat = freezed,Object? members = null,Object? linkPreview = freezed,Object? stagedAttachments = null,}) {
  return _then(_ChatDetailsState(
chat: freezed == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails?,members: null == members ? _self._members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,linkPreview: freezed == linkPreview ? _self.linkPreview : linkPreview // ignore: cast_nullable_to_non_nullable
as UiComposerLinkPreview?,stagedAttachments: null == stagedAttachments ? _self._stagedAttachments : stagedAttachments // ignore: cast_nullable_to_non_nullable
as List<UiStagedAttachment>,
  ));
}

/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiComposerLinkPreviewCopyWith<$Res>? get linkPreview {
    if (_self.linkPreview == null) {
    return null;
  }

  return $UiComposerLinkPreviewCopyWith<$Res>(_self.linkPreview!, (value) {
    return _then(_self.copyWith(linkPreview: value));
  });
}
}

/// @nodoc
mixin _$UiSendMessageError {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiSendMessageError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiSendMessageError()';
}


}

/// @nodoc
class $UiSendMessageErrorCopyWith<$Res>  {
$UiSendMessageErrorCopyWith(UiSendMessageError _, $Res Function(UiSendMessageError) __);
}



/// @nodoc


class UiSendMessageError_EditWindowPassed extends UiSendMessageError {
  const UiSendMessageError_EditWindowPassed(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiSendMessageError_EditWindowPassed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiSendMessageError.editWindowPassed()';
}


}




/// @nodoc


class UiSendMessageError_EditLimitReached extends UiSendMessageError {
  const UiSendMessageError_EditLimitReached(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiSendMessageError_EditLimitReached);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiSendMessageError.editLimitReached()';
}


}




/// @nodoc


class UiSendMessageError_AttachmentDecoding extends UiSendMessageError {
  const UiSendMessageError_AttachmentDecoding(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiSendMessageError_AttachmentDecoding);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiSendMessageError.attachmentDecoding()';
}


}




/// @nodoc


class UiSendMessageError_Other extends UiSendMessageError {
  const UiSendMessageError_Other({required this.message}): super._();
  

 final  String message;

/// Create a copy of UiSendMessageError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiSendMessageError_OtherCopyWith<UiSendMessageError_Other> get copyWith => _$UiSendMessageError_OtherCopyWithImpl<UiSendMessageError_Other>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiSendMessageError_Other&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode {
    return Object.hash(runtimeType,message);
}

@override
String toString() {
    return 'UiSendMessageError.other(message: $message)';
}


}

/// @nodoc
abstract mixin class $UiSendMessageError_OtherCopyWith<$Res> implements $UiSendMessageErrorCopyWith<$Res> {
  factory $UiSendMessageError_OtherCopyWith(UiSendMessageError_Other value, $Res Function(UiSendMessageError_Other) _then) = _$UiSendMessageError_OtherCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$UiSendMessageError_OtherCopyWithImpl<$Res>
    implements $UiSendMessageError_OtherCopyWith<$Res> {
  _$UiSendMessageError_OtherCopyWithImpl(this._self, this._then);

  final UiSendMessageError_Other _self;
  final $Res Function(UiSendMessageError_Other) _then;

/// Create a copy of UiSendMessageError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(UiSendMessageError_Other(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$UiStagedAttachment {

 AttachmentId get attachmentId; String get path; String get filename; String get contentType; BigInt get size; bool get isImage;
/// Create a copy of UiStagedAttachment
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiStagedAttachmentCopyWith<UiStagedAttachment> get copyWith => _$UiStagedAttachmentCopyWithImpl<UiStagedAttachment>(this as UiStagedAttachment, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as UiStagedAttachment;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStagedAttachment&&(identical(other.attachmentId, _this.attachmentId) || other.attachmentId == _this.attachmentId)&&(identical(other.path, _this.path) || other.path == _this.path)&&(identical(other.filename, _this.filename) || other.filename == _this.filename)&&(identical(other.contentType, _this.contentType) || other.contentType == _this.contentType)&&(identical(other.size, _this.size) || other.size == _this.size)&&(identical(other.isImage, _this.isImage) || other.isImage == _this.isImage));
}


@override
int get hashCode {
  final _this = this as UiStagedAttachment;
  return Object.hash(runtimeType,_this.attachmentId,_this.path,_this.filename,_this.contentType,_this.size,_this.isImage);
}

@override
String toString() {
  final _this = this as UiStagedAttachment;
  return 'UiStagedAttachment(attachmentId: ${_this.attachmentId}, path: ${_this.path}, filename: ${_this.filename}, contentType: ${_this.contentType}, size: ${_this.size}, isImage: ${_this.isImage})';
}


}

/// @nodoc
abstract mixin class $UiStagedAttachmentCopyWith<$Res>  {
  factory $UiStagedAttachmentCopyWith(UiStagedAttachment value, $Res Function(UiStagedAttachment) _then) = _$UiStagedAttachmentCopyWithImpl;
@useResult
$Res call({
 AttachmentId attachmentId, String path, String filename, String contentType, BigInt size, bool isImage
});




}
/// @nodoc
class _$UiStagedAttachmentCopyWithImpl<$Res>
    implements $UiStagedAttachmentCopyWith<$Res> {
  _$UiStagedAttachmentCopyWithImpl(this._self, this._then);

  final UiStagedAttachment _self;
  final $Res Function(UiStagedAttachment) _then;

/// Create a copy of UiStagedAttachment
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachmentId = null,Object? path = null,Object? filename = null,Object? contentType = null,Object? size = null,Object? isImage = null,}) {
  return _then(UiStagedAttachment(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as BigInt,isImage: null == isImage ? _self.isImage : isImage // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}



/// @nodoc


class _UiStagedAttachment implements UiStagedAttachment {
  const _UiStagedAttachment({required this.attachmentId, required this.path, required this.filename, required this.contentType, required this.size, required this.isImage});
  

@override final  AttachmentId attachmentId;
@override final  String path;
@override final  String filename;
@override final  String contentType;
@override final  BigInt size;
@override final  bool isImage;

/// Create a copy of UiStagedAttachment
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiStagedAttachmentCopyWith<_UiStagedAttachment> get copyWith => __$UiStagedAttachmentCopyWithImpl<_UiStagedAttachment>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiStagedAttachment&&(identical(other.attachmentId, attachmentId) || other.attachmentId == attachmentId)&&(identical(other.path, path) || other.path == path)&&(identical(other.filename, filename) || other.filename == filename)&&(identical(other.contentType, contentType) || other.contentType == contentType)&&(identical(other.size, size) || other.size == size)&&(identical(other.isImage, isImage) || other.isImage == isImage));
}


@override
int get hashCode {
    return Object.hash(runtimeType,attachmentId,path,filename,contentType,size,isImage);
}

@override
String toString() {
    return 'UiStagedAttachment(attachmentId: $attachmentId, path: $path, filename: $filename, contentType: $contentType, size: $size, isImage: $isImage)';
}


}

/// @nodoc
abstract mixin class _$UiStagedAttachmentCopyWith<$Res> implements $UiStagedAttachmentCopyWith<$Res> {
  factory _$UiStagedAttachmentCopyWith(_UiStagedAttachment value, $Res Function(_UiStagedAttachment) _then) = __$UiStagedAttachmentCopyWithImpl;
@override @useResult
$Res call({
 AttachmentId attachmentId, String path, String filename, String contentType, BigInt size, bool isImage
});




}
/// @nodoc
class __$UiStagedAttachmentCopyWithImpl<$Res>
    implements _$UiStagedAttachmentCopyWith<$Res> {
  __$UiStagedAttachmentCopyWithImpl(this._self, this._then);

  final _UiStagedAttachment _self;
  final $Res Function(_UiStagedAttachment) _then;

/// Create a copy of UiStagedAttachment
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachmentId = null,Object? path = null,Object? filename = null,Object? contentType = null,Object? size = null,Object? isImage = null,}) {
  return _then(_UiStagedAttachment(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,path: null == path ? _self.path : path // ignore: cast_nullable_to_non_nullable
as String,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as BigInt,isImage: null == isImage ? _self.isImage : isImage // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc
mixin _$UploadAttachmentError {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UploadAttachmentError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UploadAttachmentError()';
}


}

/// @nodoc
class $UploadAttachmentErrorCopyWith<$Res>  {
$UploadAttachmentErrorCopyWith(UploadAttachmentError _, $Res Function(UploadAttachmentError) __);
}



/// @nodoc


class UploadAttachmentError_DecodingError extends UploadAttachmentError {
  const UploadAttachmentError_DecodingError(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UploadAttachmentError_DecodingError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UploadAttachmentError.decodingError()';
}


}




/// @nodoc


class UploadAttachmentError_TooLarge extends UploadAttachmentError {
  const UploadAttachmentError_TooLarge({required this.maxSizeBytes, required this.actualSizeBytes}): super._();
  

 final  BigInt maxSizeBytes;
 final  BigInt actualSizeBytes;

/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UploadAttachmentError_TooLargeCopyWith<UploadAttachmentError_TooLarge> get copyWith => _$UploadAttachmentError_TooLargeCopyWithImpl<UploadAttachmentError_TooLarge>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UploadAttachmentError_TooLarge&&(identical(other.maxSizeBytes, maxSizeBytes) || other.maxSizeBytes == maxSizeBytes)&&(identical(other.actualSizeBytes, actualSizeBytes) || other.actualSizeBytes == actualSizeBytes));
}


@override
int get hashCode {
    return Object.hash(runtimeType,maxSizeBytes,actualSizeBytes);
}

@override
String toString() {
    return 'UploadAttachmentError.tooLarge(maxSizeBytes: $maxSizeBytes, actualSizeBytes: $actualSizeBytes)';
}


}

/// @nodoc
abstract mixin class $UploadAttachmentError_TooLargeCopyWith<$Res> implements $UploadAttachmentErrorCopyWith<$Res> {
  factory $UploadAttachmentError_TooLargeCopyWith(UploadAttachmentError_TooLarge value, $Res Function(UploadAttachmentError_TooLarge) _then) = _$UploadAttachmentError_TooLargeCopyWithImpl;
@useResult
$Res call({
 BigInt maxSizeBytes, BigInt actualSizeBytes
});




}
/// @nodoc
class _$UploadAttachmentError_TooLargeCopyWithImpl<$Res>
    implements $UploadAttachmentError_TooLargeCopyWith<$Res> {
  _$UploadAttachmentError_TooLargeCopyWithImpl(this._self, this._then);

  final UploadAttachmentError_TooLarge _self;
  final $Res Function(UploadAttachmentError_TooLarge) _then;

/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? maxSizeBytes = null,Object? actualSizeBytes = null,}) {
  return _then(UploadAttachmentError_TooLarge(
maxSizeBytes: null == maxSizeBytes ? _self.maxSizeBytes : maxSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,actualSizeBytes: null == actualSizeBytes ? _self.actualSizeBytes : actualSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

// dart format on
