// Supply chain control tower spanning planning, fulfillment, logistics, and visibility
// Mermaid mapping: complex architecture graph for cross-enterprise supply chain coordination
diagram architecture {
    title: "供应链控制塔"

    group upstream "上游与计划" {
        entity[external] supplier "Supplier Portal"
        entity[service] procurement "Procurement Service"
        entity[service] planning "Demand Planning"
        entity[service] forecast "Forecast Engine"
    }

    group operations "运营与履约" {
        entity[service] order_mgmt "Order Management"
        entity[service] inventory "Inventory Service"
        entity[service] warehouse_ops "Warehouse Operations"
        entity[service] allocation "Allocation Engine"
    }

    group logistics "物流协同" {
        entity[external] carrier "Carrier Network"
        entity[service] shipment "Shipment Service"
        entity[service] tracking "Tracking Hub"
        entity[service] eta "ETA Predictor"
    }

    group visibility "控制塔能力" {
        entity[queue] event_bus "Supply Event Bus"
        entity[frontend] control_tower "Control Tower UI"
        entity[service] alerting "Alerting Engine"
        entity[database] analytics "Operations Analytics"
    }

    supplier -> procurement
    procurement -> planning
    planning -> forecast
    forecast -> order_mgmt "replenishment plan"
    order_mgmt -> allocation
    allocation -> inventory
    inventory -> warehouse_ops
    warehouse_ops -> shipment "release shipment"
    shipment -> carrier
    carrier --> tracking "tracking updates"
    tracking -> eta
    procurement -> event_bus "po events"
    order_mgmt -> event_bus "fulfillment events"
    tracking -> event_bus "logistics events"
    event_bus -> alerting
    event_bus -> analytics
    analytics -> control_tower
    alerting -> control_tower
}
