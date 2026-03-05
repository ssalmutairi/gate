use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Result of parsing a WSDL document.
#[derive(Debug)]
pub struct WsdlParseResult {
    /// Generated OpenAPI 3.0 spec as JSON.
    pub openapi_spec: Value,
    /// SOAP metadata for the proxy (operation→SOAPAction, elements, schemas).
    pub soap_metadata: Value,
    /// The SOAP endpoint URL extracted from wsdl:service/port/soap:address.
    pub endpoint_url: String,
    /// The service name extracted from wsdl:service@name.
    pub service_name: String,
}

/// XSD type → OpenAPI type mapping.
fn xsd_to_openapi_type(xsd_type: &str) -> (&str, Option<&str>) {
    let local = xsd_type.rsplit(':').next().unwrap_or(xsd_type);
    match local {
        "string" => ("string", None),
        "int" | "integer" | "long" | "short" | "byte" | "unsignedInt" | "unsignedLong"
        | "unsignedShort" | "unsignedByte" | "nonNegativeInteger" | "positiveInteger"
        | "negativeInteger" | "nonPositiveInteger" => ("integer", None),
        "float" | "double" | "decimal" => ("number", None),
        "boolean" => ("boolean", None),
        "date" => ("string", Some("date")),
        "dateTime" => ("string", Some("date-time")),
        "base64Binary" => ("string", Some("byte")),
        _ => ("string", None),
    }
}

/// Strip namespace prefix from a tag name (e.g. "s:string" → "string").
fn strip_ns(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Parsed XSD element with its child fields.
#[derive(Debug, Clone)]
struct XsdElement {
    #[allow(dead_code)]
    name: String,
    fields: Vec<XsdField>,
}

/// A single field within an XSD complex type.
#[derive(Debug, Clone)]
struct XsdField {
    name: String,
    xsd_type: String,
}

/// A WSDL message part mapping message name → element name.
#[derive(Debug, Clone)]
struct MessagePart {
    element: String,
}

/// A WSDL operation from portType.
#[derive(Debug, Clone)]
struct PortTypeOp {
    name: String,
    input_message: String,
    output_message: String,
}

/// A WSDL binding operation with SOAPAction.
#[derive(Debug, Clone)]
struct BindingOp {
    name: String,
    soap_action: String,
}

/// Parse a WSDL XML document and produce an OpenAPI spec + SOAP metadata.
pub fn parse_wsdl(xml: &str) -> Result<WsdlParseResult, String> {
    let mut reader = Reader::from_str(xml);

    let mut target_namespace = String::new();
    let mut service_name = String::new();
    let mut endpoint_url = String::new();

    // XSD elements: element_name → XsdElement
    let mut xsd_elements: HashMap<String, XsdElement> = HashMap::new();
    // Messages: message_name → MessagePart
    let mut messages: HashMap<String, MessagePart> = HashMap::new();
    // PortType operations
    let mut port_ops: Vec<PortTypeOp> = Vec::new();
    // Binding operations
    let mut binding_ops: Vec<BindingOp> = Vec::new();

    // Parser state
    let mut in_types = false;
    let mut in_schema = false;
    let mut current_element_name: Option<String> = None;
    let mut current_element_fields: Vec<XsdField> = Vec::new();
    let mut in_complex_type = false;
    let mut in_sequence = false;
    let mut in_message = false;
    let mut current_message_name = String::new();
    let mut in_port_type = false;
    let mut current_op_name = String::new();
    let mut current_op_input = String::new();
    let mut current_op_output = String::new();
    let mut in_binding = false;
    let mut current_binding_op_name = String::new();
    let mut current_binding_soap_action = String::new();
    let mut in_binding_op = false;
    let mut in_service = false;
    let mut in_port = false;
    // Track nested element depth within schema to handle complex elements
    let mut _schema_depth: i32 = 0;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag_name).to_string();

                // Helper: get attribute value
                let get_attr = |key: &str| -> Option<String> {
                    e.attributes().filter_map(|a| a.ok()).find_map(|a| {
                        let attr_name =
                            String::from_utf8_lossy(a.key.as_ref()).to_string();
                        if strip_ns(&attr_name) == key {
                            Some(
                                String::from_utf8_lossy(&a.value).to_string(),
                            )
                        } else {
                            None
                        }
                    })
                };

                match local.as_str() {
                    "definitions" => {
                        if let Some(tns) = get_attr("targetNamespace") {
                            target_namespace = tns;
                        }
                        if let Some(name) = get_attr("name") {
                            service_name = name;
                        }
                    }
                    "types" => {
                        in_types = true;
                    }
                    "schema" if in_types => {
                        in_schema = true;
                        _schema_depth = 0;
                        if target_namespace.is_empty() {
                            if let Some(tns) = get_attr("targetNamespace") {
                                target_namespace = tns;
                            }
                        }
                    }
                    "element" if in_schema && !in_complex_type => {
                        if let Some(name) = get_attr("name") {
                            current_element_name = Some(name);
                            current_element_fields.clear();
                            _schema_depth += 1;
                        }
                    }
                    "element" if in_schema && in_sequence => {
                        // Field within a sequence
                        let field_name = get_attr("name").unwrap_or_default();
                        let field_type = get_attr("type").unwrap_or_else(|| "s:string".to_string());
                        current_element_fields.push(XsdField {
                            name: field_name,
                            xsd_type: field_type,
                        });
                    }
                    "complexType" if in_schema => {
                        in_complex_type = true;
                    }
                    "sequence" if in_complex_type => {
                        in_sequence = true;
                    }
                    "message" => {
                        in_message = true;
                        current_message_name = get_attr("name").unwrap_or_default();
                    }
                    "part" if in_message => {
                        let element = get_attr("element").unwrap_or_default();
                        let element_local = strip_ns(&element).to_string();
                        messages.insert(
                            current_message_name.clone(),
                            MessagePart {
                                element: element_local,
                            },
                        );
                    }
                    "portType" => {
                        in_port_type = true;
                    }
                    "operation" if in_port_type => {
                        current_op_name = get_attr("name").unwrap_or_default();
                        current_op_input.clear();
                        current_op_output.clear();
                    }
                    "input" if in_port_type && !current_op_name.is_empty() => {
                        let msg = get_attr("message").unwrap_or_default();
                        current_op_input = strip_ns(&msg).to_string();
                    }
                    "output" if in_port_type && !current_op_name.is_empty() => {
                        let msg = get_attr("message").unwrap_or_default();
                        current_op_output = strip_ns(&msg).to_string();
                    }
                    "binding" if !in_binding => {
                        in_binding = true;
                    }
                    "operation" if in_binding && !in_binding_op => {
                        in_binding_op = true;
                        current_binding_op_name = get_attr("name").unwrap_or_default();
                        current_binding_soap_action.clear();
                    }
                    "operation" if in_binding_op => {
                        // soap:operation
                        if let Some(action) = get_attr("soapAction") {
                            current_binding_soap_action = action;
                        }
                    }
                    "service" => {
                        in_service = true;
                        if let Some(name) = get_attr("name") {
                            service_name = name;
                        }
                    }
                    "port" if in_service => {
                        in_port = true;
                    }
                    "address" if in_port => {
                        if let Some(loc) = get_attr("location") {
                            endpoint_url = loc;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag_name).to_string();

                match local.as_str() {
                    "types" => {
                        in_types = false;
                    }
                    "schema" if in_schema => {
                        in_schema = false;
                    }
                    "element" if in_schema && !in_complex_type && current_element_name.is_some() => {
                        // Close top-level element: if it had no fields and no complex type,
                        // it's a simple element — skip it. Otherwise store.
                        if let Some(name) = current_element_name.take() {
                            if !current_element_fields.is_empty() {
                                xsd_elements.insert(
                                    name.clone(),
                                    XsdElement {
                                        name,
                                        fields: current_element_fields.clone(),
                                    },
                                );
                            }
                            current_element_fields.clear();
                            _schema_depth -= 1;
                        }
                    }
                    "complexType" if in_schema => {
                        in_complex_type = false;
                        in_sequence = false;
                    }
                    "sequence" if in_schema => {
                        in_sequence = false;
                    }
                    "message" => {
                        in_message = false;
                    }
                    "portType" => {
                        // Save the last operation if any
                        if !current_op_name.is_empty() {
                            port_ops.push(PortTypeOp {
                                name: current_op_name.clone(),
                                input_message: current_op_input.clone(),
                                output_message: current_op_output.clone(),
                            });
                            current_op_name.clear();
                        }
                        in_port_type = false;
                    }
                    "operation" if in_port_type => {
                        port_ops.push(PortTypeOp {
                            name: current_op_name.clone(),
                            input_message: current_op_input.clone(),
                            output_message: current_op_output.clone(),
                        });
                        current_op_name.clear();
                    }
                    "operation" if in_binding_op => {
                        binding_ops.push(BindingOp {
                            name: current_binding_op_name.clone(),
                            soap_action: current_binding_soap_action.clone(),
                        });
                        in_binding_op = false;
                        current_binding_op_name.clear();
                    }
                    "binding" => {
                        in_binding = false;
                        in_binding_op = false;
                    }
                    "port" => {
                        in_port = false;
                    }
                    "service" => {
                        in_service = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    if port_ops.is_empty() {
        return Err("WSDL has no operations defined in portType".into());
    }

    if endpoint_url.is_empty() {
        return Err("WSDL has no soap:address location".into());
    }

    // Build SOAPAction lookup from binding operations
    let soap_actions: HashMap<String, String> = binding_ops
        .iter()
        .map(|b| (b.name.clone(), b.soap_action.clone()))
        .collect();

    // Build OpenAPI paths and soap_metadata operations
    let mut paths = Map::new();
    let mut operations = Map::new();

    for op in &port_ops {
        let input_element_name = messages
            .get(&op.input_message)
            .map(|m| m.element.clone())
            .unwrap_or_else(|| op.name.clone());
        let output_element_name = messages
            .get(&op.output_message)
            .map(|m| m.element.clone())
            .unwrap_or_else(|| format!("{}Response", op.name));

        let soap_action = soap_actions
            .get(&op.name)
            .cloned()
            .unwrap_or_default();

        // Build JSON schemas from XSD elements
        let (input_schema_props, input_meta_schema) =
            build_schema_from_element(&xsd_elements, &input_element_name);
        let (output_schema_props, output_meta_schema) =
            build_schema_from_element(&xsd_elements, &output_element_name);

        // OpenAPI path: POST /{operation_name}
        let path = format!("/{}", op.name);
        let mut responses = Map::new();
        let mut resp_200 = Map::new();
        resp_200.insert("description".into(), json!("Successful SOAP response"));
        if !output_schema_props.is_empty() {
            resp_200.insert(
                "content".into(),
                json!({
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "properties": output_schema_props
                        }
                    }
                }),
            );
        }
        responses.insert("200".into(), Value::Object(resp_200));

        let mut post_op = Map::new();
        post_op.insert("summary".into(), json!(format!("SOAP operation: {}", op.name)));
        post_op.insert("operationId".into(), json!(op.name));
        post_op.insert("tags".into(), json!(["SOAP"]));
        post_op.insert("responses".into(), Value::Object(responses));

        if !input_schema_props.is_empty() {
            post_op.insert(
                "requestBody".into(),
                json!({
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": input_schema_props
                            }
                        }
                    }
                }),
            );
        }

        let mut path_item = Map::new();
        path_item.insert("post".into(), Value::Object(post_op));
        paths.insert(path.clone(), Value::Object(path_item));

        // SOAP metadata for this operation
        operations.insert(
            path,
            json!({
                "soap_action": soap_action,
                "operation_name": op.name,
                "input_element": input_element_name,
                "output_element": output_element_name,
                "input_schema": input_meta_schema,
                "output_schema": output_meta_schema,
            }),
        );
    }

    // Build OpenAPI spec
    let openapi_spec = json!({
        "openapi": "3.0.3",
        "info": {
            "title": service_name,
            "version": "1.0.0",
            "description": format!("Auto-generated from WSDL. SOAP endpoint: {}", endpoint_url)
        },
        "paths": paths,
    });

    let soap_metadata = json!({
        "target_namespace": target_namespace,
        "soap_endpoint": endpoint_url,
        "operations": operations,
    });

    if service_name.is_empty() {
        service_name = "SOAPService".to_string();
    }

    Ok(WsdlParseResult {
        openapi_spec,
        soap_metadata,
        endpoint_url,
        service_name,
    })
}

/// Build OpenAPI schema properties and metadata schema from an XSD element.
fn build_schema_from_element(
    elements: &HashMap<String, XsdElement>,
    element_name: &str,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut openapi_props = Map::new();
    let mut meta_schema = Map::new();

    if let Some(el) = elements.get(element_name) {
        for field in &el.fields {
            let (type_str, format) = xsd_to_openapi_type(&field.xsd_type);
            let mut prop = Map::new();
            prop.insert("type".into(), json!(type_str));
            if let Some(fmt) = format {
                prop.insert("format".into(), json!(fmt));
            }
            openapi_props.insert(field.name.clone(), Value::Object(prop.clone()));
            meta_schema.insert(field.name.clone(), json!({"type": type_str}));
        }
    }

    (openapi_props, meta_schema)
}

/// Check if content looks like a WSDL document.
pub fn is_wsdl(content: &[u8]) -> bool {
    let start = std::str::from_utf8(&content[..content.len().min(2000)])
        .unwrap_or("");
    let lower = start.to_ascii_lowercase();
    lower.contains("wsdl:definitions")
        || lower.contains("xmlns:wsdl")
        || lower.contains("schemas.xmlsoap.org/wsdl")
        || (lower.contains("<definitions") && lower.contains("wsdl"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALCULATOR_WSDL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<wsdl:definitions xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
  xmlns:tm="http://microsoft.com/wsdl/mime/textMatching/"
  xmlns:soapenc="http://schemas.xmlsoap.org/soap/encoding/"
  xmlns:mime="http://schemas.xmlsoap.org/wsdl/mime/"
  xmlns:tns="http://tempuri.org/"
  xmlns:s="http://www.w3.org/2001/XMLSchema"
  xmlns:http="http://schemas.xmlsoap.org/wsdl/http/"
  xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
  targetNamespace="http://tempuri.org/"
  name="Calculator">
  <wsdl:types>
    <s:schema elementFormDefault="qualified" targetNamespace="http://tempuri.org/">
      <s:element name="Add">
        <s:complexType>
          <s:sequence>
            <s:element minOccurs="1" maxOccurs="1" name="intA" type="s:int" />
            <s:element minOccurs="1" maxOccurs="1" name="intB" type="s:int" />
          </s:sequence>
        </s:complexType>
      </s:element>
      <s:element name="AddResponse">
        <s:complexType>
          <s:sequence>
            <s:element minOccurs="1" maxOccurs="1" name="AddResult" type="s:int" />
          </s:sequence>
        </s:complexType>
      </s:element>
      <s:element name="Subtract">
        <s:complexType>
          <s:sequence>
            <s:element minOccurs="1" maxOccurs="1" name="intA" type="s:int" />
            <s:element minOccurs="1" maxOccurs="1" name="intB" type="s:int" />
          </s:sequence>
        </s:complexType>
      </s:element>
      <s:element name="SubtractResponse">
        <s:complexType>
          <s:sequence>
            <s:element minOccurs="1" maxOccurs="1" name="SubtractResult" type="s:int" />
          </s:sequence>
        </s:complexType>
      </s:element>
    </s:schema>
  </wsdl:types>
  <wsdl:message name="AddSoapIn">
    <wsdl:part name="parameters" element="tns:Add" />
  </wsdl:message>
  <wsdl:message name="AddSoapOut">
    <wsdl:part name="parameters" element="tns:AddResponse" />
  </wsdl:message>
  <wsdl:message name="SubtractSoapIn">
    <wsdl:part name="parameters" element="tns:Subtract" />
  </wsdl:message>
  <wsdl:message name="SubtractSoapOut">
    <wsdl:part name="parameters" element="tns:SubtractResponse" />
  </wsdl:message>
  <wsdl:portType name="CalculatorSoap">
    <wsdl:operation name="Add">
      <wsdl:input message="tns:AddSoapIn" />
      <wsdl:output message="tns:AddSoapOut" />
    </wsdl:operation>
    <wsdl:operation name="Subtract">
      <wsdl:input message="tns:SubtractSoapIn" />
      <wsdl:output message="tns:SubtractSoapOut" />
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="CalculatorSoap" type="tns:CalculatorSoap">
    <soap:binding transport="http://schemas.xmlsoap.org/soap/http" />
    <wsdl:operation name="Add">
      <soap:operation soapAction="http://tempuri.org/Add" style="document" />
      <wsdl:input><soap:body use="literal" /></wsdl:input>
      <wsdl:output><soap:body use="literal" /></wsdl:output>
    </wsdl:operation>
    <wsdl:operation name="Subtract">
      <soap:operation soapAction="http://tempuri.org/Subtract" style="document" />
      <wsdl:input><soap:body use="literal" /></wsdl:input>
      <wsdl:output><soap:body use="literal" /></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="Calculator">
    <wsdl:port name="CalculatorSoap" binding="tns:CalculatorSoap">
      <soap:address location="http://www.dneonline.com/calculator.asmx" />
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    #[test]
    fn parse_calculator_wsdl() {
        let result = parse_wsdl(CALCULATOR_WSDL).expect("should parse");
        assert_eq!(result.service_name, "Calculator");
        assert_eq!(
            result.endpoint_url,
            "http://www.dneonline.com/calculator.asmx"
        );

        // Check OpenAPI has paths
        let paths = result.openapi_spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/Add"));
        assert!(paths.contains_key("/Subtract"));

        // Check soap_metadata
        let ops = result.soap_metadata["operations"].as_object().unwrap();
        let add_op = &ops["/Add"];
        assert_eq!(add_op["soap_action"], "http://tempuri.org/Add");
        assert_eq!(add_op["input_element"], "Add");
        assert_eq!(add_op["output_element"], "AddResponse");

        // Check input schema has intA and intB
        let input_schema = add_op["input_schema"].as_object().unwrap();
        assert!(input_schema.contains_key("intA"));
        assert!(input_schema.contains_key("intB"));
    }

    #[test]
    fn is_wsdl_detects_xml() {
        assert!(is_wsdl(b"<?xml version=\"1.0\"?><wsdl:definitions"));
        assert!(is_wsdl(b"<definitions xmlns=\"http://schemas.xmlsoap.org/wsdl/\""));
        assert!(is_wsdl(b"<wsdl:definitions"));
        assert!(!is_wsdl(b"{\"openapi\":\"3.0.0\"}"));
        assert!(!is_wsdl(b"{\"swagger\":\"2.0\"}"));
        // Plain XML (not WSDL) should NOT match
        assert!(!is_wsdl(b"<?xml version=\"1.0\"?><rss><channel></channel></rss>"));
        assert!(!is_wsdl(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"));
    }

    #[test]
    fn xsd_type_mapping() {
        assert_eq!(xsd_to_openapi_type("s:string"), ("string", None));
        assert_eq!(xsd_to_openapi_type("s:int"), ("integer", None));
        assert_eq!(xsd_to_openapi_type("s:float"), ("number", None));
        assert_eq!(xsd_to_openapi_type("s:boolean"), ("boolean", None));
        assert_eq!(xsd_to_openapi_type("s:dateTime"), ("string", Some("date-time")));
    }

    #[test]
    fn parse_wsdl_no_operations_error() {
        let xml = r#"<?xml version="1.0"?>
        <wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
            targetNamespace="http://test.org/" name="Empty">
            <wsdl:types></wsdl:types>
            <wsdl:portType name="EmptySoap">
            </wsdl:portType>
        </wsdl:definitions>"#;
        let result = parse_wsdl(xml);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no operations"));
    }
}
