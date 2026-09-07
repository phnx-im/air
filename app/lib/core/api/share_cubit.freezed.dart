// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'share_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ShareState {

 bool get loaded; bool get signedIn; List<UiChatDetails> get chats; UiShareSendStatus get sendStatus;
/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ShareStateCopyWith<ShareState> get copyWith => _$ShareStateCopyWithImpl<ShareState>(this as ShareState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ShareState&&(identical(other.loaded, loaded) || other.loaded == loaded)&&(identical(other.signedIn, signedIn) || other.signedIn == signedIn)&&const DeepCollectionEquality().equals(other.chats, chats)&&(identical(other.sendStatus, sendStatus) || other.sendStatus == sendStatus));
}


@override
int get hashCode => Object.hash(runtimeType,loaded,signedIn,const DeepCollectionEquality().hash(chats),sendStatus);

@override
String toString() {
  return 'ShareState(loaded: $loaded, signedIn: $signedIn, chats: $chats, sendStatus: $sendStatus)';
}


}

/// @nodoc
abstract mixin class $ShareStateCopyWith<$Res>  {
  factory $ShareStateCopyWith(ShareState value, $Res Function(ShareState) _then) = _$ShareStateCopyWithImpl;
@useResult
$Res call({
 bool loaded, bool signedIn, List<UiChatDetails> chats, UiShareSendStatus sendStatus
});


$UiShareSendStatusCopyWith<$Res> get sendStatus;

}
/// @nodoc
class _$ShareStateCopyWithImpl<$Res>
    implements $ShareStateCopyWith<$Res> {
  _$ShareStateCopyWithImpl(this._self, this._then);

  final ShareState _self;
  final $Res Function(ShareState) _then;

/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? loaded = null,Object? signedIn = null,Object? chats = null,Object? sendStatus = null,}) {
  return _then(_self.copyWith(
loaded: null == loaded ? _self.loaded : loaded // ignore: cast_nullable_to_non_nullable
as bool,signedIn: null == signedIn ? _self.signedIn : signedIn // ignore: cast_nullable_to_non_nullable
as bool,chats: null == chats ? _self.chats : chats // ignore: cast_nullable_to_non_nullable
as List<UiChatDetails>,sendStatus: null == sendStatus ? _self.sendStatus : sendStatus // ignore: cast_nullable_to_non_nullable
as UiShareSendStatus,
  ));
}
/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiShareSendStatusCopyWith<$Res> get sendStatus {
  
  return $UiShareSendStatusCopyWith<$Res>(_self.sendStatus, (value) {
    return _then(_self.copyWith(sendStatus: value));
  });
}
}



/// @nodoc


class _ShareState extends ShareState {
  const _ShareState({required this.loaded, required this.signedIn, required final  List<UiChatDetails> chats, required this.sendStatus}): _chats = chats,super._();
  

@override final  bool loaded;
@override final  bool signedIn;
 final  List<UiChatDetails> _chats;
@override List<UiChatDetails> get chats {
  if (_chats is EqualUnmodifiableListView) return _chats;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_chats);
}

@override final  UiShareSendStatus sendStatus;

/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$ShareStateCopyWith<_ShareState> get copyWith => __$ShareStateCopyWithImpl<_ShareState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _ShareState&&(identical(other.loaded, loaded) || other.loaded == loaded)&&(identical(other.signedIn, signedIn) || other.signedIn == signedIn)&&const DeepCollectionEquality().equals(other._chats, _chats)&&(identical(other.sendStatus, sendStatus) || other.sendStatus == sendStatus));
}


@override
int get hashCode => Object.hash(runtimeType,loaded,signedIn,const DeepCollectionEquality().hash(_chats),sendStatus);

@override
String toString() {
  return 'ShareState(loaded: $loaded, signedIn: $signedIn, chats: $chats, sendStatus: $sendStatus)';
}


}

/// @nodoc
abstract mixin class _$ShareStateCopyWith<$Res> implements $ShareStateCopyWith<$Res> {
  factory _$ShareStateCopyWith(_ShareState value, $Res Function(_ShareState) _then) = __$ShareStateCopyWithImpl;
@override @useResult
$Res call({
 bool loaded, bool signedIn, List<UiChatDetails> chats, UiShareSendStatus sendStatus
});


@override $UiShareSendStatusCopyWith<$Res> get sendStatus;

}
/// @nodoc
class __$ShareStateCopyWithImpl<$Res>
    implements _$ShareStateCopyWith<$Res> {
  __$ShareStateCopyWithImpl(this._self, this._then);

  final _ShareState _self;
  final $Res Function(_ShareState) _then;

/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? loaded = null,Object? signedIn = null,Object? chats = null,Object? sendStatus = null,}) {
  return _then(_ShareState(
loaded: null == loaded ? _self.loaded : loaded // ignore: cast_nullable_to_non_nullable
as bool,signedIn: null == signedIn ? _self.signedIn : signedIn // ignore: cast_nullable_to_non_nullable
as bool,chats: null == chats ? _self._chats : chats // ignore: cast_nullable_to_non_nullable
as List<UiChatDetails>,sendStatus: null == sendStatus ? _self.sendStatus : sendStatus // ignore: cast_nullable_to_non_nullable
as UiShareSendStatus,
  ));
}

/// Create a copy of ShareState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiShareSendStatusCopyWith<$Res> get sendStatus {
  
  return $UiShareSendStatusCopyWith<$Res>(_self.sendStatus, (value) {
    return _then(_self.copyWith(sendStatus: value));
  });
}
}

/// @nodoc
mixin _$UiShareSendError {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendError()';
}


}

/// @nodoc
class $UiShareSendErrorCopyWith<$Res>  {
$UiShareSendErrorCopyWith(UiShareSendError _, $Res Function(UiShareSendError) __);
}



/// @nodoc


class UiShareSendError_AttachmentTooLarge extends UiShareSendError {
  const UiShareSendError_AttachmentTooLarge({required this.maxSizeBytes, required this.actualSizeBytes}): super._();
  

 final  BigInt maxSizeBytes;
 final  BigInt actualSizeBytes;

/// Create a copy of UiShareSendError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiShareSendError_AttachmentTooLargeCopyWith<UiShareSendError_AttachmentTooLarge> get copyWith => _$UiShareSendError_AttachmentTooLargeCopyWithImpl<UiShareSendError_AttachmentTooLarge>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendError_AttachmentTooLarge&&(identical(other.maxSizeBytes, maxSizeBytes) || other.maxSizeBytes == maxSizeBytes)&&(identical(other.actualSizeBytes, actualSizeBytes) || other.actualSizeBytes == actualSizeBytes));
}


@override
int get hashCode => Object.hash(runtimeType,maxSizeBytes,actualSizeBytes);

@override
String toString() {
  return 'UiShareSendError.attachmentTooLarge(maxSizeBytes: $maxSizeBytes, actualSizeBytes: $actualSizeBytes)';
}


}

/// @nodoc
abstract mixin class $UiShareSendError_AttachmentTooLargeCopyWith<$Res> implements $UiShareSendErrorCopyWith<$Res> {
  factory $UiShareSendError_AttachmentTooLargeCopyWith(UiShareSendError_AttachmentTooLarge value, $Res Function(UiShareSendError_AttachmentTooLarge) _then) = _$UiShareSendError_AttachmentTooLargeCopyWithImpl;
@useResult
$Res call({
 BigInt maxSizeBytes, BigInt actualSizeBytes
});




}
/// @nodoc
class _$UiShareSendError_AttachmentTooLargeCopyWithImpl<$Res>
    implements $UiShareSendError_AttachmentTooLargeCopyWith<$Res> {
  _$UiShareSendError_AttachmentTooLargeCopyWithImpl(this._self, this._then);

  final UiShareSendError_AttachmentTooLarge _self;
  final $Res Function(UiShareSendError_AttachmentTooLarge) _then;

/// Create a copy of UiShareSendError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? maxSizeBytes = null,Object? actualSizeBytes = null,}) {
  return _then(UiShareSendError_AttachmentTooLarge(
maxSizeBytes: null == maxSizeBytes ? _self.maxSizeBytes : maxSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,actualSizeBytes: null == actualSizeBytes ? _self.actualSizeBytes : actualSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class UiShareSendError_TooManyAttachments extends UiShareSendError {
  const UiShareSendError_TooManyAttachments({required this.max}): super._();
  

 final  int max;

/// Create a copy of UiShareSendError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiShareSendError_TooManyAttachmentsCopyWith<UiShareSendError_TooManyAttachments> get copyWith => _$UiShareSendError_TooManyAttachmentsCopyWithImpl<UiShareSendError_TooManyAttachments>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendError_TooManyAttachments&&(identical(other.max, max) || other.max == max));
}


@override
int get hashCode => Object.hash(runtimeType,max);

@override
String toString() {
  return 'UiShareSendError.tooManyAttachments(max: $max)';
}


}

/// @nodoc
abstract mixin class $UiShareSendError_TooManyAttachmentsCopyWith<$Res> implements $UiShareSendErrorCopyWith<$Res> {
  factory $UiShareSendError_TooManyAttachmentsCopyWith(UiShareSendError_TooManyAttachments value, $Res Function(UiShareSendError_TooManyAttachments) _then) = _$UiShareSendError_TooManyAttachmentsCopyWithImpl;
@useResult
$Res call({
 int max
});




}
/// @nodoc
class _$UiShareSendError_TooManyAttachmentsCopyWithImpl<$Res>
    implements $UiShareSendError_TooManyAttachmentsCopyWith<$Res> {
  _$UiShareSendError_TooManyAttachmentsCopyWithImpl(this._self, this._then);

  final UiShareSendError_TooManyAttachments _self;
  final $Res Function(UiShareSendError_TooManyAttachments) _then;

/// Create a copy of UiShareSendError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? max = null,}) {
  return _then(UiShareSendError_TooManyAttachments(
max: null == max ? _self.max : max // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class UiShareSendError_Other extends UiShareSendError {
  const UiShareSendError_Other(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendError_Other);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendError.other()';
}


}




/// @nodoc
mixin _$UiShareSendStatus {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendStatus()';
}


}

/// @nodoc
class $UiShareSendStatusCopyWith<$Res>  {
$UiShareSendStatusCopyWith(UiShareSendStatus _, $Res Function(UiShareSendStatus) __);
}



/// @nodoc


class UiShareSendStatus_Idle extends UiShareSendStatus {
  const UiShareSendStatus_Idle(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Idle);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendStatus.idle()';
}


}




/// @nodoc


class UiShareSendStatus_Uploading extends UiShareSendStatus {
  const UiShareSendStatus_Uploading({required this.current, required this.total, required this.progress}): super._();
  

/// 1-based index of the attachment currently uploading.
 final  int current;
/// Total number of attachments to upload.
 final  int total;
/// Overall progress over all attachments in `0.0..=1.0`.
 final  double progress;

/// Create a copy of UiShareSendStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiShareSendStatus_UploadingCopyWith<UiShareSendStatus_Uploading> get copyWith => _$UiShareSendStatus_UploadingCopyWithImpl<UiShareSendStatus_Uploading>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Uploading&&(identical(other.current, current) || other.current == current)&&(identical(other.total, total) || other.total == total)&&(identical(other.progress, progress) || other.progress == progress));
}


@override
int get hashCode => Object.hash(runtimeType,current,total,progress);

@override
String toString() {
  return 'UiShareSendStatus.uploading(current: $current, total: $total, progress: $progress)';
}


}

/// @nodoc
abstract mixin class $UiShareSendStatus_UploadingCopyWith<$Res> implements $UiShareSendStatusCopyWith<$Res> {
  factory $UiShareSendStatus_UploadingCopyWith(UiShareSendStatus_Uploading value, $Res Function(UiShareSendStatus_Uploading) _then) = _$UiShareSendStatus_UploadingCopyWithImpl;
@useResult
$Res call({
 int current, int total, double progress
});




}
/// @nodoc
class _$UiShareSendStatus_UploadingCopyWithImpl<$Res>
    implements $UiShareSendStatus_UploadingCopyWith<$Res> {
  _$UiShareSendStatus_UploadingCopyWithImpl(this._self, this._then);

  final UiShareSendStatus_Uploading _self;
  final $Res Function(UiShareSendStatus_Uploading) _then;

/// Create a copy of UiShareSendStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? current = null,Object? total = null,Object? progress = null,}) {
  return _then(UiShareSendStatus_Uploading(
current: null == current ? _self.current : current // ignore: cast_nullable_to_non_nullable
as int,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as int,progress: null == progress ? _self.progress : progress // ignore: cast_nullable_to_non_nullable
as double,
  ));
}


}

/// @nodoc


class UiShareSendStatus_Sending extends UiShareSendStatus {
  const UiShareSendStatus_Sending(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Sending);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendStatus.sending()';
}


}




/// @nodoc


class UiShareSendStatus_Done extends UiShareSendStatus {
  const UiShareSendStatus_Done(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Done);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendStatus.done()';
}


}




/// @nodoc


class UiShareSendStatus_Queued extends UiShareSendStatus {
  const UiShareSendStatus_Queued(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Queued);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiShareSendStatus.queued()';
}


}




/// @nodoc


class UiShareSendStatus_Failed extends UiShareSendStatus {
  const UiShareSendStatus_Failed({required this.error}): super._();
  

 final  UiShareSendError error;

/// Create a copy of UiShareSendStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiShareSendStatus_FailedCopyWith<UiShareSendStatus_Failed> get copyWith => _$UiShareSendStatus_FailedCopyWithImpl<UiShareSendStatus_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiShareSendStatus_Failed&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode => Object.hash(runtimeType,error);

@override
String toString() {
  return 'UiShareSendStatus.failed(error: $error)';
}


}

/// @nodoc
abstract mixin class $UiShareSendStatus_FailedCopyWith<$Res> implements $UiShareSendStatusCopyWith<$Res> {
  factory $UiShareSendStatus_FailedCopyWith(UiShareSendStatus_Failed value, $Res Function(UiShareSendStatus_Failed) _then) = _$UiShareSendStatus_FailedCopyWithImpl;
@useResult
$Res call({
 UiShareSendError error
});


$UiShareSendErrorCopyWith<$Res> get error;

}
/// @nodoc
class _$UiShareSendStatus_FailedCopyWithImpl<$Res>
    implements $UiShareSendStatus_FailedCopyWith<$Res> {
  _$UiShareSendStatus_FailedCopyWithImpl(this._self, this._then);

  final UiShareSendStatus_Failed _self;
  final $Res Function(UiShareSendStatus_Failed) _then;

/// Create a copy of UiShareSendStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? error = null,}) {
  return _then(UiShareSendStatus_Failed(
error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as UiShareSendError,
  ));
}

/// Create a copy of UiShareSendStatus
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiShareSendErrorCopyWith<$Res> get error {
  
  return $UiShareSendErrorCopyWith<$Res>(_self.error, (value) {
    return _then(_self.copyWith(error: value));
  });
}
}

// dart format on
