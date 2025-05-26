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
        
        for key, value in partial_data.items():
            # Check if this field should be a specific type
            if key in annotations:
                field_type = annotations[key]
                
                # Handle list of objects
                if hasattr(field_type, "__origin__") and field_type.__origin__ is list:
                    # Get the element type of the list
                    if hasattr(field_type, "__args__") and len(field_type.__args__) > 0:
                        elem_type = field_type.__args__[0]
                        
                        # If element type is a Deserializable subclass and value is a list
                        if (issubclass(elem_type, Deserializable) and 
                            isinstance(value, list)):
                            # Convert each item in the list to the proper type
                            typed_list = []
                            for item in value:
                                # If it's already the right type, use it directly
                                if isinstance(item, elem_type):
                                    typed_list.append(item)
                                # Otherwise, if it's a dict, convert it
                                elif isinstance(item, dict):
                                    typed_list.append(elem_type.__gasp_from_partial__(item))
                                else:
                                    # Use the item as is
                                    typed_list.append(item)
                            
                            # Set the properly typed list
                            setattr(instance, key, typed_list)
                            continue
                
                # Handle single nested object
                # Need to check if field_type is actually a class before calling issubclass
                try:
                    if isinstance(field_type, type) and issubclass(field_type, Deserializable) and isinstance(value, dict):
                        setattr(instance, key, field_type.__gasp_from_partial__(value))
                        continue
                except TypeError:
                    # field_type is not a class, skip the check
                    pass
                    
            # Default case - set value directly
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
    
    def model_dump(self):
        """Convert model to dict (Pydantic V2 compatible)"""
        result = {}
        for k, v in self.__dict__.items():
            if k.startswith('_'):
                continue
                
            # Recursively dump nested Deserializable objects
            if isinstance(v, Deserializable):
                result[k] = v.model_dump()
            # Handle lists that might contain Deserializable objects
            elif isinstance(v, list):
                dumped_list = []
                for item in v:
                    if isinstance(item, Deserializable):
                        dumped_list.append(item.model_dump())
                    else:
                        dumped_list.append(item)
                result[k] = dumped_list
            # Handle dictionaries that might contain Deserializable objects
            elif isinstance(v, dict):
                dumped_dict = {}
                for dict_k, dict_v in v.items():
                    if isinstance(dict_v, Deserializable):
                        dumped_dict[dict_k] = dict_v.model_dump()
                    else:
                        dumped_dict[dict_k] = dict_v
                result[k] = dumped_dict
            else:
                result[k] = v
                
        return result
