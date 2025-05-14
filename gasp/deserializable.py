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
        for key, value in partial_data.items():
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
        return {k: v for k, v in self.__dict__.items() if not k.startswith('_')}
