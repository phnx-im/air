// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'user_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$VersionStatus {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VersionStatus);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'VersionStatus()';
}


}

/// @nodoc
class $VersionStatusCopyWith<$Res>  {
$VersionStatusCopyWith(VersionStatus _, $Res Function(VersionStatus) __);
}



/// @nodoc


class VersionStatus_Supported extends VersionStatus {
  const VersionStatus_Supported(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VersionStatus_Supported);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'VersionStatus.supported()';
}


}




/// @nodoc


class VersionStatus_Unsupported extends VersionStatus {
  const VersionStatus_Unsupported(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VersionStatus_Unsupported);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'VersionStatus.unsupported()';
}


}




/// @nodoc


class VersionStatus_ExpiresAt extends VersionStatus {
  const VersionStatus_ExpiresAt(this.field0): super._();
  

 final  DateTime field0;

/// Create a copy of VersionStatus
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$VersionStatus_ExpiresAtCopyWith<VersionStatus_ExpiresAt> get copyWith => _$VersionStatus_ExpiresAtCopyWithImpl<VersionStatus_ExpiresAt>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is VersionStatus_ExpiresAt&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'VersionStatus.expiresAt(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $VersionStatus_ExpiresAtCopyWith<$Res> implements $VersionStatusCopyWith<$Res> {
  factory $VersionStatus_ExpiresAtCopyWith(VersionStatus_ExpiresAt value, $Res Function(VersionStatus_ExpiresAt) _then) = _$VersionStatus_ExpiresAtCopyWithImpl;
@useResult
$Res call({
 DateTime field0
});




}
/// @nodoc
class _$VersionStatus_ExpiresAtCopyWithImpl<$Res>
    implements $VersionStatus_ExpiresAtCopyWith<$Res> {
  _$VersionStatus_ExpiresAtCopyWithImpl(this._self, this._then);

  final VersionStatus_ExpiresAt _self;
  final $Res Function(VersionStatus_ExpiresAt) _then;

/// Create a copy of VersionStatus
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(VersionStatus_ExpiresAt(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as DateTime,
  ));
}


}

// dart format on
