"""
Deserializable base class for GASP typed object deserialization.
"""

class Deserializable:
    """Base class for types that can be deserialized from JSON"""
    
    @classmethod
    def __gasp_register__(cls):
        """Register the type for deserialization"""
        pass
    
    @classmethod
    def __gasp_from_partial__(cls, partial_data):
        """Create an instance from partial data"""
        instance = cls()
        
        # Get type annotations to check for nested types
        annotations = getattr(cls, "__annotations__", {})
        
        # Initialize all annotated fields with appropriate defaults
        for field_name, field_type in annotations.items():
            if field_name not in partial_data:
                # Check if the class has a default value for this field
                if hasattr(cls, field_name):
                    # Use the class-level default
                    default_value = getattr(cls, field_name)
                    setattr(instance, field_name, default_value)
                # Otherwise set default values based on type
                elif hasattr(field_type, "__origin__"):
                    if field_type.__origin__ is list:
                        setattr(instance, field_name, [])
                    elif field_type.__origin__ is dict:
                        setattr(instance, field_name, {})
                    elif field_type.__origin__ is set:
                        setattr(instance, field_name, set())
                    elif field_type.__origin__ is tuple:
                        setattr(instance, field_name, ())
                    # For Optional/Union types, set to None
                    elif hasattr(field_type, "__args__") and type(None) in field_type.__args__:
                        setattr(instance, field_name, None)
                else:
                    # For regular types, set to None (caller should handle required fields)
                    setattr(instance, field_name, None)
        
        for key, value in partial_data.items():
            # Don't overwrite an already-set meaningful value (prevents "Engineering" overriding "TechCorp")
            current_val = getattr(instance, key, None)
            if current_val not in (None, [], {}, (), set()):
                continue

            if key in annotations:
                field_type = annotations[key]

                # Handle list[...] of Deserializable
                if getattr(field_type, "__origin__", None) is list:
                    elem_type = getattr(field_type, "__args__", [None])[0]
                    if (isinstance(elem_type, type) and issubclass(elem_type, Deserializable)
                            and isinstance(value, list)):
                        typed_list = [
                            item if isinstance(item, elem_type)
                            else elem_type.__gasp_from_partial__(item) if isinstance(item, dict)
                            else item
                            for item in value
                        ]
                        setattr(instance, key, typed_list)
                        continue

                # Handle dict[str, Deserializable]
                if getattr(field_type, "__origin__", None) is dict:
                    val_type = getattr(field_type, "__args__", [None, None])[1]
                    if (isinstance(val_type, type) and issubclass(val_type, Deserializable)
                            and isinstance(value, dict)):
                        typed_dict = {
                            k: v if isinstance(v, val_type)
                            else val_type.__gasp_from_partial__(v) if isinstance(v, dict)
                            else v
                            for k, v in value.items()
                        }
                        setattr(instance, key, typed_dict)
                        continue

                # Handle single nested Deserializable
                try:
                    if isinstance(field_type, type) and issubclass(field_type, Deserializable) and isinstance(value, dict):
                        setattr(instance, key, field_type.__gasp_from_partial__(value))
                        continue
                except TypeError:
                    pass  # field_type is not a class

            # Fallback – set value directly
            setattr(instance, key, value)
        
        return instance
    
    def __gasp_update__(self, new_data):
        """Update instance with new data"""
        for key, value in new_data.items():
            setattr(self, key, value)
    
    # Pydantic V2 compatibility methods
    @classmethod
    def model_validate(cls, obj):
        """Pydantic V2 compatible validation method"""
        return cls.__gasp_from_partial__(obj)
    
    @classmethod
    def model_fields(cls):
        """Return field information compatible with Pydantic V2"""
        fields = {}
        for name, type_hint in getattr(cls, "__annotations__", {}).items():
            fields[name] = {"type": type_hint}
        return fields
    
    def model_dump(self, exclude_none=True, mode="dict"):
        """Convert model to dict (Pydantic V2 compatible)"""
        result = {}
        for k, v in self.__dict__.items():
            if k.startswith('_'):
                continue

            # Exclude fields with value None if exclude_none is True
            if exclude_none and v is None:
                continue

            # Recursively dump nested Deserializable objects
            if isinstance(v, Deserializable):
                dumped = v.model_dump(exclude_none=exclude_none)
                if not (exclude_none and dumped is None):
                    result[k] = dumped
            # Handle lists that might contain Deserializable objects
            elif isinstance(v, list):
                dumped_list = []
                for item in v:
                    if isinstance(item, Deserializable):
                        dumped_item = item.model_dump(exclude_none=exclude_none, mode=mode)
                        if not (exclude_none and dumped_item is None):
                            dumped_list.append(dumped_item)
                    else:
                        if not (exclude_none and item is None):
                            dumped_list.append(item)
                result[k] = dumped_list
            # Handle dictionaries that might contain Deserializable objects
            elif isinstance(v, dict):
                dumped_dict = {}
                for dict_k, dict_v in v.items():
                    if isinstance(dict_v, Deserializable):
                        dumped_item = dict_v.model_dump(exclude_none=exclude_none, mode=mode)
                        if not (exclude_none and dumped_item is None):
                            dumped_dict[dict_k] = dumped_item
                    else:
                        if not (exclude_none and dict_v is None):
                            dumped_dict[dict_k] = dict_v
                result[k] = dumped_dict
            else:
                result[k] = v

        return result
    
    def model_dump_json(self):
        """Convert model to JSON string (Pydantic V2 compatible)"""
        import json
        return json.dumps(self.model_dump(mode="json"), ensure_ascii=False, indent=2)
