// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'multi_device.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$MultiDeviceLinkEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceLinkEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MultiDeviceLinkEvent()';
}


}

/// @nodoc
class $MultiDeviceLinkEventCopyWith<$Res>  {
$MultiDeviceLinkEventCopyWith(MultiDeviceLinkEvent _, $Res Function(MultiDeviceLinkEvent) __);
}



/// @nodoc


class MultiDeviceLinkEvent_AwaitingConfirmation extends MultiDeviceLinkEvent {
  const MultiDeviceLinkEvent_AwaitingConfirmation(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceLinkEvent_AwaitingConfirmation);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MultiDeviceLinkEvent.awaitingConfirmation()';
}


}




/// @nodoc


class MultiDeviceLinkEvent_Linked extends MultiDeviceLinkEvent {
  const MultiDeviceLinkEvent_Linked(this.field0): super._();
  

 final  String field0;

/// Create a copy of MultiDeviceLinkEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MultiDeviceLinkEvent_LinkedCopyWith<MultiDeviceLinkEvent_Linked> get copyWith => _$MultiDeviceLinkEvent_LinkedCopyWithImpl<MultiDeviceLinkEvent_Linked>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceLinkEvent_Linked&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'MultiDeviceLinkEvent.linked(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $MultiDeviceLinkEvent_LinkedCopyWith<$Res> implements $MultiDeviceLinkEventCopyWith<$Res> {
  factory $MultiDeviceLinkEvent_LinkedCopyWith(MultiDeviceLinkEvent_Linked value, $Res Function(MultiDeviceLinkEvent_Linked) _then) = _$MultiDeviceLinkEvent_LinkedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$MultiDeviceLinkEvent_LinkedCopyWithImpl<$Res>
    implements $MultiDeviceLinkEvent_LinkedCopyWith<$Res> {
  _$MultiDeviceLinkEvent_LinkedCopyWithImpl(this._self, this._then);

  final MultiDeviceLinkEvent_Linked _self;
  final $Res Function(MultiDeviceLinkEvent_Linked) _then;

/// Create a copy of MultiDeviceLinkEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(MultiDeviceLinkEvent_Linked(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class MultiDeviceLinkEvent_Failed extends MultiDeviceLinkEvent {
  const MultiDeviceLinkEvent_Failed(this.field0): super._();
  

 final  String field0;

/// Create a copy of MultiDeviceLinkEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MultiDeviceLinkEvent_FailedCopyWith<MultiDeviceLinkEvent_Failed> get copyWith => _$MultiDeviceLinkEvent_FailedCopyWithImpl<MultiDeviceLinkEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceLinkEvent_Failed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'MultiDeviceLinkEvent.failed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $MultiDeviceLinkEvent_FailedCopyWith<$Res> implements $MultiDeviceLinkEventCopyWith<$Res> {
  factory $MultiDeviceLinkEvent_FailedCopyWith(MultiDeviceLinkEvent_Failed value, $Res Function(MultiDeviceLinkEvent_Failed) _then) = _$MultiDeviceLinkEvent_FailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$MultiDeviceLinkEvent_FailedCopyWithImpl<$Res>
    implements $MultiDeviceLinkEvent_FailedCopyWith<$Res> {
  _$MultiDeviceLinkEvent_FailedCopyWithImpl(this._self, this._then);

  final MultiDeviceLinkEvent_Failed _self;
  final $Res Function(MultiDeviceLinkEvent_Failed) _then;

/// Create a copy of MultiDeviceLinkEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(MultiDeviceLinkEvent_Failed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$MultiDeviceProvisionEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceProvisionEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MultiDeviceProvisionEvent()';
}


}

/// @nodoc
class $MultiDeviceProvisionEventCopyWith<$Res>  {
$MultiDeviceProvisionEventCopyWith(MultiDeviceProvisionEvent _, $Res Function(MultiDeviceProvisionEvent) __);
}



/// @nodoc


class MultiDeviceProvisionEvent_Code extends MultiDeviceProvisionEvent {
  const MultiDeviceProvisionEvent_Code({this.qrcodeSvg, required this.code}): super._();
  

 final  String? qrcodeSvg;
 final  String code;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MultiDeviceProvisionEvent_CodeCopyWith<MultiDeviceProvisionEvent_Code> get copyWith => _$MultiDeviceProvisionEvent_CodeCopyWithImpl<MultiDeviceProvisionEvent_Code>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceProvisionEvent_Code&&(identical(other.qrcodeSvg, qrcodeSvg) || other.qrcodeSvg == qrcodeSvg)&&(identical(other.code, code) || other.code == code));
}


@override
int get hashCode => Object.hash(runtimeType,qrcodeSvg,code);

@override
String toString() {
  return 'MultiDeviceProvisionEvent.code(qrcodeSvg: $qrcodeSvg, code: $code)';
}


}

/// @nodoc
abstract mixin class $MultiDeviceProvisionEvent_CodeCopyWith<$Res> implements $MultiDeviceProvisionEventCopyWith<$Res> {
  factory $MultiDeviceProvisionEvent_CodeCopyWith(MultiDeviceProvisionEvent_Code value, $Res Function(MultiDeviceProvisionEvent_Code) _then) = _$MultiDeviceProvisionEvent_CodeCopyWithImpl;
@useResult
$Res call({
 String? qrcodeSvg, String code
});




}
/// @nodoc
class _$MultiDeviceProvisionEvent_CodeCopyWithImpl<$Res>
    implements $MultiDeviceProvisionEvent_CodeCopyWith<$Res> {
  _$MultiDeviceProvisionEvent_CodeCopyWithImpl(this._self, this._then);

  final MultiDeviceProvisionEvent_Code _self;
  final $Res Function(MultiDeviceProvisionEvent_Code) _then;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? qrcodeSvg = freezed,Object? code = null,}) {
  return _then(MultiDeviceProvisionEvent_Code(
qrcodeSvg: freezed == qrcodeSvg ? _self.qrcodeSvg : qrcodeSvg // ignore: cast_nullable_to_non_nullable
as String?,code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class MultiDeviceProvisionEvent_Linking extends MultiDeviceProvisionEvent {
  const MultiDeviceProvisionEvent_Linking(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceProvisionEvent_Linking);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MultiDeviceProvisionEvent.linking()';
}


}




/// @nodoc


class MultiDeviceProvisionEvent_Linked extends MultiDeviceProvisionEvent {
  const MultiDeviceProvisionEvent_Linked(this.field0): super._();
  

 final  String field0;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MultiDeviceProvisionEvent_LinkedCopyWith<MultiDeviceProvisionEvent_Linked> get copyWith => _$MultiDeviceProvisionEvent_LinkedCopyWithImpl<MultiDeviceProvisionEvent_Linked>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceProvisionEvent_Linked&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'MultiDeviceProvisionEvent.linked(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $MultiDeviceProvisionEvent_LinkedCopyWith<$Res> implements $MultiDeviceProvisionEventCopyWith<$Res> {
  factory $MultiDeviceProvisionEvent_LinkedCopyWith(MultiDeviceProvisionEvent_Linked value, $Res Function(MultiDeviceProvisionEvent_Linked) _then) = _$MultiDeviceProvisionEvent_LinkedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$MultiDeviceProvisionEvent_LinkedCopyWithImpl<$Res>
    implements $MultiDeviceProvisionEvent_LinkedCopyWith<$Res> {
  _$MultiDeviceProvisionEvent_LinkedCopyWithImpl(this._self, this._then);

  final MultiDeviceProvisionEvent_Linked _self;
  final $Res Function(MultiDeviceProvisionEvent_Linked) _then;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(MultiDeviceProvisionEvent_Linked(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class MultiDeviceProvisionEvent_Failed extends MultiDeviceProvisionEvent {
  const MultiDeviceProvisionEvent_Failed(this.field0): super._();
  

 final  String field0;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MultiDeviceProvisionEvent_FailedCopyWith<MultiDeviceProvisionEvent_Failed> get copyWith => _$MultiDeviceProvisionEvent_FailedCopyWithImpl<MultiDeviceProvisionEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MultiDeviceProvisionEvent_Failed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'MultiDeviceProvisionEvent.failed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $MultiDeviceProvisionEvent_FailedCopyWith<$Res> implements $MultiDeviceProvisionEventCopyWith<$Res> {
  factory $MultiDeviceProvisionEvent_FailedCopyWith(MultiDeviceProvisionEvent_Failed value, $Res Function(MultiDeviceProvisionEvent_Failed) _then) = _$MultiDeviceProvisionEvent_FailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$MultiDeviceProvisionEvent_FailedCopyWithImpl<$Res>
    implements $MultiDeviceProvisionEvent_FailedCopyWith<$Res> {
  _$MultiDeviceProvisionEvent_FailedCopyWithImpl(this._self, this._then);

  final MultiDeviceProvisionEvent_Failed _self;
  final $Res Function(MultiDeviceProvisionEvent_Failed) _then;

/// Create a copy of MultiDeviceProvisionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(MultiDeviceProvisionEvent_Failed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
