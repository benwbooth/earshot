#include "earshot/native.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

void earshotSetWindowIcon(const QString& resource)
{
    QGuiApplication::setWindowIcon(QIcon(resource));
}
